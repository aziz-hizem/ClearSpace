//! The in-memory file tree shared by every ClearSpace scanner.
//!
//! Design goals: millions of entries in modest memory, cheap appends from many scanner
//! threads (behind one short-held lock), and O(n) size aggregation.
//!
//! * Nodes live in one `Vec<Node>` arena and are addressed by [`NodeId`] (a `u32`).
//!   Each node is 48 bytes; names live in a separate byte pool.
//! * A directory's children are stored **contiguously** (`first_child..first_child +
//!   child_count`), because scanners read a whole directory before appending it. This makes
//!   child iteration a slice walk and keeps the tree compact.
//! * A child always has a higher index than its parent, so one reverse pass over the arena
//!   aggregates sizes bottom-up ([`Tree::aggregate`]).
//! * Names are stored with [`OsStr::as_encoded_bytes`], so Windows names that are not valid
//!   Unicode (unpaired surrogates) round-trip exactly — paths can always be reopened.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod csv;
mod ext;
pub mod time;

use std::collections::TryReserveError;
use std::ffi::OsStr;
use std::fmt;
use std::path::PathBuf;

pub use ext::{EXT_NONE, EXT_OTHER, ExtTable};

/// Sentinel for "no node".
const NONE: u32 = u32::MAX;

/// Index of a node in a [`Tree`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

impl NodeId {
    /// The scan root is always node 0.
    pub const ROOT: NodeId = NodeId(0);

    /// Raw index, e.g. for IPC. Only meaningful for the tree it came from.
    pub fn get(self) -> u32 {
        self.0
    }

    /// Rebuilds an id from [`NodeId::get`]. Callers must bounds-check with [`Tree::contains`].
    pub fn from_raw(raw: u32) -> NodeId {
        NodeId(raw)
    }

    fn idx(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Bit flags stored on every node.
pub mod flags {
    /// The node is a directory (possibly not yet listed).
    pub const DIR: u16 = 1 << 0;
    /// A symlink, junction or mount point. Recorded, never followed, counts zero bytes.
    pub const LINK: u16 = 1 << 1;
    /// A cloud placeholder (OneDrive Files On-Demand etc.). Content may not be local; never
    /// read it (that would download it).
    pub const CLOUD: u16 = 1 << 2;
    /// Another hard link to a file already counted elsewhere in this scan; counts zero bytes.
    pub const HARDLINK_DUP: u16 = 1 << 3;
    pub const COMPRESSED: u16 = 1 << 4;
    pub const SPARSE: u16 = 1 << 5;
    /// The directory could not be listed (or only partially). See the scan's issue list.
    pub const UNREADABLE: u16 = 1 << 6;
    pub const HIDDEN: u16 = 1 << 7;
    pub const SYSTEM: u16 = 1 << 8;
    /// The directory's children have been recorded.
    pub const LISTED: u16 = 1 << 9;
    pub const READONLY: u16 = 1 << 10;
}

/// One file-system entry. 48 bytes; see the size assertion in the tests.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Node {
    /// Logical size in bytes. For directories, the aggregated total after
    /// [`Tree::aggregate`].
    pub size: u64,
    /// Allocated size on disk ("size on disk"). Aggregated for directories.
    pub alloc: u64,
    parent: u32,
    first_child: u32,
    child_count: u32,
    name_off: u32,
    /// Last-write time in whole minutes since the Unix epoch (0 = unknown). See [`time`].
    pub mtime: u32,
    /// Directories: number of files beneath (after aggregation). Files: 0.
    pub files: u32,
    name_len: u16,
    /// [`flags`] bits.
    pub flags: u16,
    /// Extension id from [`Tree::exts`]; [`EXT_NONE`] for directories.
    pub ext: u16,
    _reserved: u16,
}

impl Node {
    pub fn is_dir(&self) -> bool {
        self.flags & flags::DIR != 0
    }
    pub fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }
    pub fn child_count(&self) -> u32 {
        self.child_count
    }
    /// Whether this node's bytes are included in its parent's totals.
    fn counts_bytes(&self) -> bool {
        self.flags & (flags::LINK | flags::HARDLINK_DUP) == 0
    }
}

/// An entry to append under a directory with [`Tree::push_children`].
#[derive(Clone, Copy, Debug)]
pub struct NewEntry<'a> {
    pub name: &'a OsStr,
    pub flags: u16,
    pub size: u64,
    pub alloc: u64,
    pub mtime: u32,
}

/// The tree has hit a hard limit (4 billion nodes, 4 GiB of names) or the allocator refused
/// to grow it. Scanners stop and keep the partial result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapacityError {
    TooManyNodes,
    NamesTooLarge,
    OutOfMemory,
}

impl From<TryReserveError> for CapacityError {
    fn from(_: TryReserveError) -> Self {
        CapacityError::OutOfMemory
    }
}

impl fmt::Display for CapacityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CapacityError::TooManyNodes => "too many entries for one scan",
            CapacityError::NamesTooLarge => "file names exceed the scan's name storage",
            CapacityError::OutOfMemory => "not enough memory to continue the scan",
        })
    }
}

impl std::error::Error for CapacityError {}

/// An arena-allocated file tree rooted at a scan root.
pub struct Tree {
    nodes: Vec<Node>,
    names: Vec<u8>,
    exts: ExtTable,
}

impl fmt::Debug for Tree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tree")
            .field("nodes", &self.nodes.len())
            .field("name_bytes", &self.names.len())
            .field("exts", &self.exts.len())
            .finish()
    }
}

impl Tree {
    /// Creates a tree whose root is the directory `root` (stored as its full display path,
    /// e.g. `C:\` or `D:\Projects`).
    pub fn new(root: &OsStr) -> Tree {
        let mut tree = Tree {
            nodes: Vec::new(),
            names: Vec::new(),
            exts: ExtTable::new(),
        };
        let bytes = root.as_encoded_bytes();
        let name_len = u16::try_from(bytes.len()).unwrap_or(u16::MAX);
        tree.names.extend_from_slice(&bytes[..name_len as usize]);
        tree.nodes.push(Node {
            size: 0,
            alloc: 0,
            parent: NONE,
            first_child: NONE,
            child_count: 0,
            name_off: 0,
            mtime: 0,
            files: 0,
            name_len,
            flags: flags::DIR,
            ext: EXT_NONE,
            _reserved: 0,
        });
        tree
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn contains(&self, id: NodeId) -> bool {
        id.idx() < self.nodes.len()
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.idx()]
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        let p = self.nodes[id.idx()].parent;
        (p != NONE).then_some(NodeId(p))
    }

    pub fn name(&self, id: NodeId) -> &OsStr {
        let n = &self.nodes[id.idx()];
        let bytes = &self.names[n.name_off as usize..n.name_off as usize + n.name_len as usize];
        // SAFETY: every name in the pool was copied whole from `OsStr::as_encoded_bytes` in
        // this process (see `Tree::new` / `push_children`), which is exactly the contract of
        // `from_encoded_bytes_unchecked`. Names are never split or concatenated in the pool.
        unsafe { OsStr::from_encoded_bytes_unchecked(bytes) }
    }

    /// Full path of a node, built from the root's display path.
    pub fn path(&self, id: NodeId) -> PathBuf {
        let mut chain = Vec::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            chain.push(c);
            cur = self.parent(c);
        }
        let mut path = PathBuf::new();
        for c in chain.into_iter().rev() {
            path.push(self.name(c));
        }
        path
    }

    /// Depth below the root (root = 0).
    pub fn depth(&self, id: NodeId) -> usize {
        let mut d = 0;
        let mut cur = id;
        while let Some(p) = self.parent(cur) {
            d += 1;
            cur = p;
        }
        d
    }

    pub fn children(&self, id: NodeId) -> impl ExactSizeIterator<Item = NodeId> + use<> {
        let n = &self.nodes[id.idx()];
        let start = if n.child_count == 0 { 0 } else { n.first_child };
        (start..start + n.child_count).map(NodeId)
    }

    pub fn exts(&self) -> &ExtTable {
        &self.exts
    }

    /// Bytes of heap memory held by the tree (approximate; for diagnostics and the UI).
    pub fn memory_bytes(&self) -> usize {
        self.nodes.capacity() * std::mem::size_of::<Node>()
            + self.names.capacity()
            + self.exts.memory_bytes()
    }

    /// Releases spare capacity once a scan is finished.
    pub fn shrink_to_fit(&mut self) {
        self.nodes.shrink_to_fit();
        self.names.shrink_to_fit();
    }

    /// Appends the full listing of directory `parent`. Must be called at most once per
    /// directory, with all of its entries, so that children stay contiguous.
    ///
    /// On error nothing is appended and the tree remains valid.
    pub fn push_children<'a, I>(&mut self, parent: NodeId, entries: I) -> Result<(), CapacityError>
    where
        I: IntoIterator<Item = NewEntry<'a>>,
        I::IntoIter: ExactSizeIterator + Clone,
    {
        let entries = entries.into_iter();
        let pidx = parent.idx();
        assert!(self.nodes[pidx].is_dir(), "push_children on a file node");
        assert!(
            !self.nodes[pidx].has(flags::LISTED),
            "push_children called twice for one directory"
        );
        let count = entries.len();
        let first = self.nodes.len();
        if first + count >= NONE as usize {
            return Err(CapacityError::TooManyNodes);
        }
        let name_bytes: usize = entries
            .clone()
            .map(|e| e.name.as_encoded_bytes().len().min(u16::MAX as usize))
            .sum();
        if self.names.len() + name_bytes > u32::MAX as usize {
            return Err(CapacityError::NamesTooLarge);
        }
        self.nodes.try_reserve(count)?;
        self.names.try_reserve(name_bytes)?;

        for e in entries {
            let bytes = e.name.as_encoded_bytes();
            // Windows component names are at most 255 UTF-16 units, so this never truncates
            // real names; the clamp only protects against corrupt input.
            let bytes = &bytes[..bytes.len().min(u16::MAX as usize)];
            let name_off = self.names.len() as u32;
            self.names.extend_from_slice(bytes);
            let is_dir = e.flags & flags::DIR != 0;
            let ext = if is_dir {
                EXT_NONE
            } else {
                self.exts.intern_from_name(bytes)
            };
            self.nodes.push(Node {
                size: if is_dir { 0 } else { e.size },
                alloc: if is_dir { 0 } else { e.alloc },
                parent: parent.0,
                first_child: NONE,
                child_count: 0,
                name_off,
                mtime: e.mtime,
                files: 0,
                name_len: bytes.len() as u16,
                flags: e.flags & !flags::LISTED,
                ext,
                _reserved: 0,
            });
        }
        let p = &mut self.nodes[pidx];
        p.first_child = if count == 0 { NONE } else { first as u32 };
        p.child_count = count as u32;
        p.flags |= flags::LISTED;
        Ok(())
    }

    /// Sets flag bits on a node (e.g. [`flags::UNREADABLE`] after a listing error).
    pub fn set_flags(&mut self, id: NodeId, bits: u16) {
        self.nodes[id.idx()].flags |= bits;
    }

    /// Recomputes every directory's `size`, `alloc` and `files` from its descendants.
    /// O(n), allocation-free. Safe to call repeatedly on a partial tree.
    pub fn aggregate(&mut self) {
        for n in self.nodes.iter_mut().filter(|n| n.is_dir()) {
            n.size = 0;
            n.alloc = 0;
            n.files = 0;
        }
        for i in (1..self.nodes.len()).rev() {
            let n = self.nodes[i];
            let (size, alloc, files) = match (n.is_dir(), n.counts_bytes()) {
                (true, true) => (n.size, n.alloc, n.files),
                (false, true) => (n.size, n.alloc, 1),
                (false, false) if n.has(flags::HARDLINK_DUP) => (0, 0, 1),
                _ => (0, 0, 0),
            };
            let p = &mut self.nodes[n.parent as usize];
            p.size += size;
            p.alloc += alloc;
            p.files += files;
        }
    }

    /// Children of `id` ordered by descending size (ties broken by name for stable output).
    pub fn children_by_size(&self, id: NodeId) -> Vec<NodeId> {
        let mut v: Vec<NodeId> = self.children(id).collect();
        v.sort_by(|a, b| {
            self.node(*b)
                .size
                .cmp(&self.node(*a).size)
                .then_with(|| self.name(*a).cmp(self.name(*b)))
        });
        v
    }

    /// The `n` largest files (by logical size) in the whole tree, largest first.
    pub fn largest_files(&self, n: usize) -> Vec<NodeId> {
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;
        let mut heap: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::with_capacity(n + 1);
        for (i, node) in self.nodes.iter().enumerate() {
            if node.is_dir() || !node.counts_bytes() {
                continue;
            }
            heap.push(Reverse((node.size, i as u32)));
            if heap.len() > n {
                heap.pop();
            }
        }
        let mut v: Vec<(u64, u32)> = heap.into_iter().map(|Reverse(x)| x).collect();
        v.sort_by(|a, b| b.cmp(a));
        v.into_iter().map(|(_, i)| NodeId(i)).collect()
    }

    /// Totals per file extension, largest first.
    pub fn extension_breakdown(&self) -> Vec<ExtStat> {
        let mut stats = vec![ExtStat::default(); self.exts.len()];
        for node in &self.nodes {
            if node.is_dir() || !node.counts_bytes() {
                continue;
            }
            let s = &mut stats[self.exts.slot(node.ext)];
            s.ext = node.ext;
            s.size += node.size;
            s.alloc += node.alloc;
            s.files += 1;
        }
        stats.retain(|s| s.files > 0);
        stats.sort_by_key(|s| std::cmp::Reverse(s.size));
        stats
    }

    /// Looks up a descendant of the root by path components (case-sensitive). Test helper and
    /// debugging aid; the UI navigates by [`NodeId`].
    pub fn find<S: AsRef<OsStr>>(&self, components: &[S]) -> Option<NodeId> {
        let mut cur = NodeId::ROOT;
        for c in components {
            cur = self.children(cur).find(|&k| self.name(k) == c.as_ref())?;
        }
        Some(cur)
    }
}

/// Aggregated totals for one extension.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExtStat {
    pub ext: u16,
    pub size: u64,
    pub alloc: u64,
    pub files: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file<'a>(name: &'a str, size: u64) -> NewEntry<'a> {
        NewEntry {
            name: OsStr::new(name),
            flags: 0,
            size,
            alloc: size.div_ceil(4096) * 4096,
            mtime: 0,
        }
    }
    fn dir(name: &str) -> NewEntry<'_> {
        NewEntry {
            name: OsStr::new(name),
            flags: flags::DIR,
            size: 0,
            alloc: 0,
            mtime: 0,
        }
    }

    #[test]
    fn node_is_48_bytes() {
        assert_eq!(std::mem::size_of::<Node>(), 48);
    }

    fn sample() -> Tree {
        let mut t = Tree::new(OsStr::new("ROOT"));
        t.push_children(NodeId::ROOT, [dir("a"), file("x.TXT", 10), dir("b")])
            .unwrap();
        let a = t.find(&["a"]).unwrap();
        t.push_children(a, [file("big.bin", 5000), file("small.txt", 1)])
            .unwrap();
        let b = t.find(&["b"]).unwrap();
        let link = NewEntry {
            flags: flags::DIR | flags::LINK,
            ..dir("junction")
        };
        let dup = NewEntry {
            flags: flags::HARDLINK_DUP,
            ..file("dup.bin", 5000)
        };
        t.push_children(b, [link, dup]).unwrap();
        t.aggregate();
        t
    }

    #[test]
    fn aggregates_sizes_and_counts() {
        let t = sample();
        let root = t.node(NodeId::ROOT);
        assert_eq!(root.size, 5011);
        assert_eq!(root.files, 4, "dup counts as an entry but not as bytes");
        assert_eq!(t.node(t.find(&["a"]).unwrap()).size, 5001);
        assert_eq!(
            t.node(t.find(&["b"]).unwrap()).size,
            0,
            "links and hardlink dups add 0"
        );
    }

    #[test]
    fn aggregate_is_idempotent() {
        let mut t = sample();
        let before = t.node(NodeId::ROOT).size;
        t.aggregate();
        t.aggregate();
        assert_eq!(t.node(NodeId::ROOT).size, before);
    }

    #[test]
    fn children_are_contiguous_and_paths_round_trip() {
        let t = sample();
        let names: Vec<_> = t
            .children(NodeId::ROOT)
            .map(|c| t.name(c).to_owned())
            .collect();
        assert_eq!(names, ["a", "x.TXT", "b"]);
        let big = t.find(&["a", "big.bin"]).unwrap();
        assert_eq!(t.path(big), PathBuf::from("ROOT").join("a").join("big.bin"));
        assert_eq!(t.depth(big), 2);
        assert_eq!(t.parent(big), t.find(&["a"]));
    }

    #[test]
    fn queries() {
        let t = sample();
        let largest: Vec<_> = t
            .largest_files(2)
            .iter()
            .map(|&i| t.name(i).to_owned())
            .collect();
        assert_eq!(largest, ["big.bin", "x.TXT"], "hardlink dup excluded");
        let by_size: Vec<_> = t
            .children_by_size(NodeId::ROOT)
            .iter()
            .map(|&i| t.name(i).to_owned())
            .collect();
        assert_eq!(by_size, ["a", "x.TXT", "b"]);
        let exts = t.extension_breakdown();
        assert_eq!(t.exts().name(exts[0].ext), "bin");
        let txt = exts.iter().find(|s| t.exts().name(s.ext) == "txt").unwrap();
        assert_eq!((txt.files, txt.size), (2, 11), "extensions are case-folded");
    }

    #[test]
    #[should_panic(expected = "twice")]
    fn listing_a_directory_twice_is_a_bug() {
        let mut t = Tree::new(OsStr::new("R"));
        t.push_children(NodeId::ROOT, [file("a", 1)]).unwrap();
        t.push_children(NodeId::ROOT, [file("b", 1)]).unwrap();
    }

    #[test]
    fn empty_directory() {
        let mut t = Tree::new(OsStr::new("R"));
        t.push_children(NodeId::ROOT, [dir("empty")]).unwrap();
        let e = t.find(&["empty"]).unwrap();
        t.push_children(e, std::iter::empty::<NewEntry>().collect::<Vec<_>>())
            .unwrap();
        assert_eq!(t.children(e).len(), 0);
        assert!(t.node(e).has(flags::LISTED));
    }

    #[test]
    fn unicode_names_round_trip() {
        let mut t = Tree::new(OsStr::new("R"));
        t.push_children(
            NodeId::ROOT,
            [file("日本語 ファイル.txt", 3), file("emoji-😀.png", 4)],
        )
        .unwrap();
        assert!(t.find(&["日本語 ファイル.txt"]).is_some());
        assert!(t.find(&["emoji-😀.png"]).is_some());
    }

    #[cfg(windows)]
    #[test]
    fn unpaired_surrogate_names_round_trip() {
        use std::ffi::OsString;
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let raw: Vec<u16> = vec![b'a' as u16, 0xD800, b'.' as u16, b't' as u16];
        let name = OsString::from_wide(&raw);
        let mut t = Tree::new(OsStr::new("R"));
        t.push_children(
            NodeId::ROOT,
            [NewEntry {
                name: &name,
                flags: 0,
                size: 1,
                alloc: 1,
                mtime: 0,
            }],
        )
        .unwrap();
        let id = t.children(NodeId::ROOT).next().unwrap();
        assert_eq!(t.name(id).encode_wide().collect::<Vec<_>>(), raw);
    }
}
