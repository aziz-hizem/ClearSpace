//! The parallel scan engine.
//!
//! One background thread owns a dedicated rayon pool; every directory becomes a task that
//! lists the directory (no lock held), then appends the whole listing to the shared tree in
//! one short critical section and spawns tasks for its sub-directories. Callers poll
//! [`ScanHandle::progress`], can look at the partial tree at any time, and can cancel.

use std::collections::HashSet;
use std::ffi::OsString;
use std::hash::{BuildHasherDefault, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use clearspace_model::{CapacityError, NewEntry, NodeId, Tree, flags};

use crate::source::{EntryKind, FsSource, ListError, ListErrorKind};

/// Scan tuning.
#[derive(Clone, Debug)]
pub struct ScanOptions {
    /// Worker threads. Directory listing is I/O-bound; SSDs benefit from parallelism.
    pub threads: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        let cpus = std::thread::available_parallelism().map_or(4, |n| n.get());
        ScanOptions {
            threads: cpus.clamp(2, 8),
        }
    }
}

/// How a scan ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanStatus {
    Running,
    Completed,
    Cancelled,
    /// The drive disappeared mid-scan. The partial tree is kept.
    VolumeLost,
    /// Ran out of memory or tree capacity. The partial tree is kept.
    OutOfMemory(CapacityError),
    /// The root itself could not be opened.
    RootUnavailable(ListError),
}

impl ScanStatus {
    /// Whether the tree covers everything that was readable.
    pub fn is_complete(&self) -> bool {
        matches!(self, ScanStatus::Completed)
    }
}

/// A folder that could not be read (fully). Reported to the user, never fatal (except
/// [`ListErrorKind::VolumeGone`], which ends the scan).
#[derive(Clone, Debug)]
pub struct Issue {
    pub node: NodeId,
    pub path: PathBuf,
    pub error: ListError,
    /// True if some entries were read before the failure.
    pub partial: bool,
}

/// Live counters, cheap to read at any time.
#[derive(Clone, Debug)]
pub struct Progress {
    pub files: u64,
    pub dirs: u64,
    /// Logical bytes found so far (hard-link duplicates and links excluded).
    pub bytes: u64,
    /// Allocated bytes found so far.
    pub alloc: u64,
    pub issues: u64,
    pub elapsed: Duration,
    /// A recently scanned folder, for "Scanning C:\Users\…" style display.
    pub current: Option<PathBuf>,
    pub status: ScanStatus,
}

/// Final result of a scan.
#[derive(Debug)]
pub struct ScanOutcome {
    /// Aggregated tree (folder totals are up to date).
    pub tree: Tree,
    pub issues: Vec<Issue>,
    pub status: ScanStatus,
    pub progress: Progress,
}

#[derive(Default)]
struct IdHasher(u64);

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        // splitmix64 finaliser: file ids are sequential-ish, spread them out.
        let mut z = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(b);
        }
    }
    fn write_u128(&mut self, v: u128) {
        self.0 = (v as u64) ^ ((v >> 64) as u64).rotate_left(32);
    }
}

type IdSet = HashSet<u128, BuildHasherDefault<IdHasher>>;
const ID_SHARDS: usize = 64;

struct Shared<S> {
    source: S,
    tree: Mutex<Tree>,
    issues: Mutex<Vec<Issue>>,
    /// File ids already counted, to count hard-linked files once. Sharded to limit
    /// contention; dropped when the scan ends.
    seen_ids: Vec<Mutex<IdSet>>,
    cancel: Arc<AtomicBool>,
    status: Mutex<ScanStatus>,
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    alloc: AtomicU64,
    issue_count: AtomicU64,
    current: Mutex<Option<PathBuf>>,
    started: Instant,
    finished_at: Mutex<Option<Duration>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic in one worker must not take the whole scan (and the UI) down with it.
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl<S: FsSource> Shared<S> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Records a terminal condition (first one wins) and stops all workers.
    fn stop(&self, status: ScanStatus) {
        let mut s = lock(&self.status);
        if *s == ScanStatus::Running {
            *s = status;
        }
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn first_sighting(&self, id: u128) -> bool {
        let shard = (id as usize ^ (id >> 64) as usize) % ID_SHARDS;
        lock(&self.seen_ids[shard]).insert(id)
    }

    fn report(&self, node: NodeId, path: &Path, error: ListError, partial: bool) {
        self.issue_count.fetch_add(1, Ordering::Relaxed);
        lock(&self.tree).set_flags(node, flags::UNREADABLE);
        lock(&self.issues).push(Issue {
            node,
            path: path.to_owned(),
            error,
            partial,
        });
    }

    fn progress(&self) -> Progress {
        let elapsed = lock(&self.finished_at).unwrap_or_else(|| self.started.elapsed());
        Progress {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            alloc: self.alloc.load(Ordering::Relaxed),
            issues: self.issue_count.load(Ordering::Relaxed),
            elapsed,
            current: lock(&self.current).clone(),
            status: lock(&self.status).clone(),
        }
    }
}

/// A cheap, clonable handle that can stop a scan from anywhere (UI thread, Ctrl+C handler).
#[derive(Clone, Debug)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// A running (or finished) scan.
pub struct ScanHandle<S: FsSource> {
    shared: Arc<Shared<S>>,
    thread: Option<JoinHandle<()>>,
}

impl<S: FsSource> std::fmt::Debug for ScanHandle<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanHandle")
            .field("progress", &self.progress())
            .finish()
    }
}

/// Starts scanning `root` in the background. `root` should be an absolute display path
/// (see [`crate::paths::normalize_root`]).
pub fn start<S: FsSource>(source: S, root: PathBuf, options: ScanOptions) -> ScanHandle<S> {
    let shared = Arc::new(Shared {
        source,
        tree: Mutex::new(Tree::new(root.as_os_str())),
        issues: Mutex::new(Vec::new()),
        seen_ids: (0..ID_SHARDS)
            .map(|_| Mutex::new(IdSet::default()))
            .collect(),
        cancel: Arc::new(AtomicBool::new(false)),
        status: Mutex::new(ScanStatus::Running),
        files: AtomicU64::new(0),
        dirs: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
        alloc: AtomicU64::new(0),
        issue_count: AtomicU64::new(0),
        current: Mutex::new(None),
        started: Instant::now(),
        finished_at: Mutex::new(None),
    });
    let worker = Arc::clone(&shared);
    let thread = std::thread::Builder::new()
        .name("clearspace-scan".into())
        .spawn(move || run(worker, root, options))
        .expect("failed to spawn scan thread");
    ScanHandle {
        shared,
        thread: Some(thread),
    }
}

fn run<S: FsSource>(shared: Arc<Shared<S>>, root: PathBuf, options: ScanOptions) {
    match shared.source.open_root(&root) {
        Err(e) => shared.stop(ScanStatus::RootUnavailable(e)),
        Ok(root_id) => {
            shared.dirs.fetch_add(1, Ordering::Relaxed);
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(options.threads.max(1))
                .thread_name(|i| format!("clearspace-scan-{i}"))
                .build();
            match pool {
                Ok(pool) => pool.scope(|scope| {
                    scan_dir(scope, &shared, NodeId::ROOT, root, root_id);
                }),
                Err(_) => scan_dir_inline(&shared, NodeId::ROOT, root, root_id),
            }
        }
    }
    {
        let mut status = lock(&shared.status);
        if *status == ScanStatus::Running {
            *status = if shared.cancelled() {
                ScanStatus::Cancelled
            } else {
                ScanStatus::Completed
            };
        }
    }
    {
        let mut tree = lock(&shared.tree);
        tree.aggregate();
        tree.shrink_to_fit();
    }
    *lock(&shared.finished_at) = Some(shared.started.elapsed());
    // Free the hard-link bookkeeping now; it can be large.
    for shard in &shared.seen_ids {
        *lock(shard) = IdSet::default();
    }
}

/// Lists one directory, appends it to the tree and returns its sub-directories to visit.
fn visit<S: FsSource>(
    shared: &Shared<S>,
    node: NodeId,
    path: &Path,
    expected_id: Option<u128>,
) -> Vec<(NodeId, OsString, Option<u128>)> {
    if shared.cancelled() {
        return Vec::new();
    }
    let listing = match shared.source.list(path, expected_id) {
        Ok(l) => l,
        Err(e) => {
            let fatal = e.kind == ListErrorKind::VolumeGone;
            shared.report(node, path, e, false);
            if fatal {
                shared.stop(ScanStatus::VolumeLost);
            }
            return Vec::new();
        }
    };

    let mut entry_flags = Vec::with_capacity(listing.entries.len());
    let (mut files, mut dirs, mut bytes, mut alloc) = (0u64, 0u64, 0u64, 0u64);
    for e in &listing.entries {
        let mut f = e.flags & !(flags::DIR | flags::LINK | flags::LISTED);
        match e.kind {
            EntryKind::Dir => {
                f |= flags::DIR;
                dirs += 1;
            }
            EntryKind::Link { dir } => {
                // Recorded so the UI can show it, never descended into, counts zero bytes.
                f |= flags::LINK;
                if dir {
                    f |= flags::DIR;
                }
            }
            EntryKind::File => {
                files += 1;
                let dup =
                    e.maybe_hardlink && e.file_id.is_some_and(|id| !shared.first_sighting(id));
                if dup {
                    f |= flags::HARDLINK_DUP;
                } else {
                    bytes += e.size;
                    alloc += e.alloc;
                }
            }
        }
        entry_flags.push(f);
    }

    let first_child = {
        let mut tree = lock(&shared.tree);
        let entries = listing
            .entries
            .iter()
            .zip(&entry_flags)
            .map(|(e, &f)| NewEntry {
                name: &e.name,
                flags: f,
                size: e.size,
                alloc: e.alloc,
                mtime: e.mtime,
            });
        if let Err(cap) = tree.push_children(node, entries) {
            drop(tree);
            shared.stop(ScanStatus::OutOfMemory(cap));
            return Vec::new();
        }
        tree.children(node).next().map(|c| c.get())
    };

    shared.files.fetch_add(files, Ordering::Relaxed);
    shared.dirs.fetch_add(dirs, Ordering::Relaxed);
    shared.bytes.fetch_add(bytes, Ordering::Relaxed);
    shared.alloc.fetch_add(alloc, Ordering::Relaxed);
    if shared.dirs.load(Ordering::Relaxed).is_multiple_of(256) {
        *lock(&shared.current) = Some(path.to_owned());
    }
    if let Some(err) = listing.partial {
        shared.report(node, path, err, true);
    }

    let Some(first) = first_child else {
        return Vec::new();
    };
    listing
        .entries
        .into_iter()
        .enumerate()
        .filter(|(_, e)| e.kind == EntryKind::Dir)
        .map(|(i, e)| (NodeId::from_raw(first + i as u32), e.name, e.file_id))
        .collect()
}

fn scan_dir<'s, S: FsSource>(
    scope: &rayon::Scope<'s>,
    shared: &'s Arc<Shared<S>>,
    node: NodeId,
    path: PathBuf,
    expected_id: Option<u128>,
) {
    for (child, name, id) in visit(shared, node, &path, expected_id) {
        let child_path = path.join(&name);
        scope.spawn(move |s| scan_dir(s, shared, child, child_path, id));
    }
}

/// Single-threaded fallback if the thread pool cannot be created (e.g. low resources).
fn scan_dir_inline<S: FsSource>(
    shared: &Shared<S>,
    node: NodeId,
    path: PathBuf,
    expected_id: Option<u128>,
) {
    let mut stack = vec![(node, path, expected_id)];
    while let Some((node, path, id)) = stack.pop() {
        for (child, name, cid) in visit(shared, node, &path, id) {
            stack.push((child, path.join(&name), cid));
        }
    }
}

impl<S: FsSource> ScanHandle<S> {
    pub fn progress(&self) -> Progress {
        self.shared.progress()
    }

    /// Requests cancellation. Workers stop after their current directory; the partial tree
    /// is kept and the final status is [`ScanStatus::Cancelled`].
    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
    }

    pub fn cancel_token(&self) -> CancelToken {
        CancelToken(Arc::clone(&self.shared.cancel))
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }

    /// Runs `f` on the current (possibly partial) tree with folder totals brought up to
    /// date. Briefly pauses tree appends; intended for a few UI refreshes per second.
    pub fn with_tree<R>(&self, f: impl FnOnce(&Tree) -> R) -> R {
        let mut tree = lock(&self.shared.tree);
        tree.aggregate();
        f(&tree)
    }

    /// Waits for the scan to finish and returns the result.
    pub fn wait(mut self) -> ScanOutcome {
        if let Some(t) = self.thread.take() {
            // A panicking scan thread still leaves a valid (partial) tree behind.
            if t.join().is_err() {
                self.shared.stop(ScanStatus::Cancelled);
                lock(&self.shared.tree).aggregate();
            }
        }
        let progress = self.shared.progress();
        let tree = std::mem::replace(&mut *lock(&self.shared.tree), Tree::new("".as_ref()));
        let issues = std::mem::take(&mut *lock(&self.shared.issues));
        ScanOutcome {
            tree,
            issues,
            status: progress.status.clone(),
            progress,
        }
    }
}

impl<S: FsSource> Drop for ScanHandle<S> {
    fn drop(&mut self) {
        if let Some(t) = self.thread.take() {
            self.shared.cancel.store(true, Ordering::Relaxed);
            let _ = t.join();
        }
    }
}

/// Convenience: scan to completion on the calling thread's behalf.
pub fn scan_blocking<S: FsSource>(source: S, root: PathBuf, options: ScanOptions) -> ScanOutcome {
    start(source, root, options).wait()
}
