//! An in-memory file system for tests: build a tree, inject failures (access denied, folders
//! vanishing, the drive disappearing, slow folders) and scan it like a real one.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::source::{EntryKind, FsSource, ListError, ListErrorKind, Listing, RawEntry};

#[derive(Debug, Default)]
struct FakeDir {
    entries: Vec<RawEntry>,
    fail: Option<ListErrorKind>,
    fail_after: Option<(usize, ListErrorKind)>,
    id: u128,
}

/// Builder and [`FsSource`] for an in-memory tree. Paths use `/` and are relative to the
/// root given to [`FakeFs::new`].
#[derive(Debug)]
pub struct FakeFs {
    root: PathBuf,
    dirs: HashMap<PathBuf, FakeDir>,
    next_id: u128,
    delay: Option<Duration>,
}

impl FakeFs {
    pub fn new(root: impl Into<PathBuf>) -> FakeFs {
        let root = root.into();
        let mut fs = FakeFs {
            root: root.clone(),
            dirs: HashMap::new(),
            next_id: 1,
            delay: None,
        };
        fs.dirs.insert(
            root,
            FakeDir {
                id: 0,
                ..Default::default()
            },
        );
        fs
    }

    fn abs(&self, rel: &str) -> PathBuf {
        rel.split('/')
            .filter(|c| !c.is_empty())
            .fold(self.root.clone(), |p, c| p.join(c))
    }

    fn ensure_dir(&mut self, rel: &str) -> PathBuf {
        let path = self.abs(rel);
        if self.dirs.contains_key(&path) {
            return path;
        }
        let (parent, name) = split(rel);
        let parent_path = self.ensure_dir(parent);
        let id = self.alloc_id();
        self.dirs.get_mut(&parent_path).unwrap().entries.push(entry(
            name,
            EntryKind::Dir,
            0,
            Some(id),
        ));
        self.dirs.insert(
            path.clone(),
            FakeDir {
                id,
                ..Default::default()
            },
        );
        path
    }

    fn alloc_id(&mut self) -> u128 {
        self.next_id += 1;
        self.next_id
    }

    fn push(&mut self, rel: &str, e: RawEntry) -> &mut RawEntry {
        let (parent, _) = split(rel);
        let parent_path = self.ensure_dir(parent);
        let list = &mut self.dirs.get_mut(&parent_path).unwrap().entries;
        list.push(e);
        list.last_mut().unwrap()
    }

    pub fn dir(mut self, rel: &str) -> Self {
        self.ensure_dir(rel);
        self
    }

    pub fn file(mut self, rel: &str, size: u64) -> Self {
        let id = self.alloc_id();
        let (_, name) = split(rel);
        self.push(rel, entry(name, EntryKind::File, size, Some(id)));
        self
    }

    /// A file with extra model flags (e.g. `flags::CLOUD`) and a custom allocated size.
    pub fn file_with(mut self, rel: &str, size: u64, alloc: u64, flags: u16) -> Self {
        let id = self.alloc_id();
        let (_, name) = split(rel);
        let e = self.push(rel, entry(name, EntryKind::File, size, Some(id)));
        e.alloc = alloc;
        e.flags = flags;
        self
    }

    /// A hard link: another name for the file with id `link_id` (use any shared number).
    pub fn hardlink(mut self, rel: &str, size: u64, link_id: u128) -> Self {
        let (_, name) = split(rel);
        let e = self.push(
            rel,
            entry(name, EntryKind::File, size, Some(1 << 100 | link_id)),
        );
        e.maybe_hardlink = true;
        self
    }

    /// A junction/symlink to a directory. Its target is a real directory elsewhere; the
    /// scanner must not descend into it.
    pub fn link_dir(mut self, rel: &str) -> Self {
        let (_, name) = split(rel);
        self.push(rel, entry(name, EntryKind::Link { dir: true }, 0, None));
        self
    }

    /// Listing this directory fails with `kind`.
    pub fn failing(mut self, rel: &str, kind: ListErrorKind) -> Self {
        let p = self.ensure_dir(rel);
        self.dirs.get_mut(&p).unwrap().fail = Some(kind);
        self
    }

    /// Listing this directory returns only the first `n` entries, then fails with `kind`.
    pub fn failing_after(mut self, rel: &str, n: usize, kind: ListErrorKind) -> Self {
        let p = self.ensure_dir(rel);
        self.dirs.get_mut(&p).unwrap().fail_after = Some((n, kind));
        self
    }

    /// The directory's identity changes after discovery (e.g. replaced by a junction).
    pub fn swapped(mut self, rel: &str) -> Self {
        let p = self.ensure_dir(rel);
        self.dirs.get_mut(&p).unwrap().id ^= 0xDEAD;
        self
    }

    /// Every listing sleeps this long (for cancellation tests).
    pub fn slow(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
}

fn split(rel: &str) -> (&str, &str) {
    let rel = rel.trim_end_matches('/');
    match rel.rfind('/') {
        Some(i) => (&rel[..i], &rel[i + 1..]),
        None => ("", rel),
    }
}

fn entry(name: &str, kind: EntryKind, size: u64, id: Option<u128>) -> RawEntry {
    RawEntry {
        name: OsString::from(name),
        kind,
        size,
        alloc: size.div_ceil(4096) * 4096,
        mtime: 29_000_000,
        flags: 0,
        file_id: id,
        maybe_hardlink: false,
    }
}

impl FsSource for FakeFs {
    fn open_root(&self, root: &Path) -> Result<Option<u128>, ListError> {
        match self.dirs.get(root) {
            Some(d) if d.fail.is_none() => Ok(Some(d.id)),
            Some(d) => Err(ListError::new(d.fail.unwrap())),
            None => Err(ListError::new(ListErrorKind::NotFound)),
        }
    }

    fn list(&self, dir: &Path, expected_id: Option<u128>) -> Result<Listing, ListError> {
        if let Some(d) = self.delay {
            std::thread::sleep(d);
        }
        let d = self
            .dirs
            .get(dir)
            .ok_or(ListError::new(ListErrorKind::NotFound))?;
        if let Some(kind) = d.fail {
            return Err(ListError::new(kind));
        }
        // Mirror the real sources: the id recorded in the parent listing must still match.
        if expected_id.is_some_and(|id| id != d.id) && d.id != 0 {
            return Err(ListError::new(ListErrorKind::Changed));
        }
        let mut entries = d.entries.clone();
        let partial = d.fail_after.map(|(n, kind)| {
            entries.truncate(n);
            ListError::new(kind)
        });
        Ok(Listing { entries, partial })
    }
}

/// Builds a large synthetic tree quickly (for scale tests): `dirs` folders each holding
/// `files_per_dir` files.
pub fn wide_tree(root: &str, dirs: usize, files_per_dir: usize) -> FakeFs {
    let mut fs = FakeFs::new(root);
    for d in 0..dirs {
        let dir = format!("d{}/sub{}", d % 100, d);
        fs = fs.dir(&dir);
        let p = fs.abs(&dir);
        let mut next = fs.next_id;
        let list = &mut fs.dirs.get_mut(&p).unwrap().entries;
        for f in 0..files_per_dir {
            next += 1;
            list.push(entry(
                &format!("file{f}.dat"),
                EntryKind::File,
                1000 + f as u64,
                Some(next),
            ));
        }
        fs.next_id = next;
    }
    fs
}
