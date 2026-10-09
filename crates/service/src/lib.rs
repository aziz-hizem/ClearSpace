//! Scan sessions and the read-only queries the UI makes against them.
//!
//! This crate is the boundary between the scanner/model and the UI layer. It deliberately has
//! no UI-framework dependency, so every query is unit-tested off-Windows; the Tauri crate is a
//! thin IPC wrapper around it.
//!
//! The UI never sends file paths for actions: it sends `(scan id, node id)` pairs and this
//! crate resolves them against its own tree. That keeps a compromised or buggy frontend from
//! pointing an action at an arbitrary path.
//!
//! Nothing here deletes or modifies anything.

pub mod dto;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use clearspace_model::filetype::{Category, category_of};
use clearspace_model::{NodeId, Tree, flags};
use clearspace_scanner::scan::{ScanHandle, ScanOptions, ScanOutcome, ScanStatus, start};
use clearspace_scanner::{CancelToken, FsSource, paths::normalize_root};
use clearspace_treemap as treemap;

pub use dto::*;

/// Errors returned to the UI. Messages are plain language.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "code", content = "message", rename_all = "camelCase")]
pub enum ServiceError {
    UnknownScan(String),
    UnknownNode(String),
    BadRequest(String),
    Io(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::UnknownScan(m)
            | ServiceError::UnknownNode(m)
            | ServiceError::BadRequest(m)
            | ServiceError::Io(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for ServiceError {}

pub type Result<T> = std::result::Result<T, ServiceError>;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

enum State<S: FsSource> {
    Running(ScanHandle<S>),
    Finished(Box<ScanOutcome>),
    /// Momentarily empty while moving from `Running` to `Finished`.
    Moving,
}

/// One scan and its (possibly still growing) tree.
pub struct Session<S: FsSource> {
    pub id: u32,
    pub root: PathBuf,
    state: Mutex<State<S>>,
    cancel: CancelToken,
}

impl<S: FsSource> std::fmt::Debug for Session<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("id", &self.id)
            .field("root", &self.root)
            .finish()
    }
}

impl<S: FsSource> Session<S> {
    /// Current progress. Also completes the hand-over to the finished state once the scan
    /// thread is done, so callers should poll this until `status != "running"`.
    pub fn poll(&self) -> ProgressDto {
        let mut st = lock(&self.state);
        if matches!(&*st, State::Running(h) if h.is_finished()) {
            let State::Running(h) = std::mem::replace(&mut *st, State::Moving) else {
                unreachable!()
            };
            *st = State::Finished(Box::new(h.wait()));
        }
        match &*st {
            State::Running(h) => ProgressDto::from_progress(self.id, &self.root, &h.progress()),
            State::Finished(o) => ProgressDto::from_progress(self.id, &self.root, &o.progress),
            State::Moving => unreachable!("state is never left Moving"),
        }
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Runs `f` on the current tree (aggregated). Works during and after the scan.
    pub fn with_tree<R>(&self, f: impl FnOnce(&Tree) -> R) -> R {
        let st = lock(&self.state);
        match &*st {
            State::Running(h) => h.with_tree(f),
            State::Finished(o) => f(&o.tree),
            State::Moving => unreachable!("state is never left Moving"),
        }
    }

    /// Folders that could not be read (first `limit`).
    pub fn issues(&self, limit: usize) -> Vec<IssueDto> {
        let st = lock(&self.state);
        match &*st {
            State::Finished(o) => o.issues.iter().take(limit).map(IssueDto::from).collect(),
            _ => Vec::new(),
        }
    }
}

/// All scan sessions of the running app.
pub struct ScanService<S: FsSource> {
    sessions: Mutex<HashMap<u32, Arc<Session<S>>>>,
    next_id: AtomicU32,
    options: ScanOptions,
}

impl<S: FsSource> std::fmt::Debug for ScanService<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanService")
            .field("sessions", &lock(&self.sessions).len())
            .finish()
    }
}

impl<S: FsSource> Default for ScanService<S> {
    fn default() -> Self {
        ScanService::new(ScanOptions::default())
    }
}

impl<S: FsSource> ScanService<S> {
    pub fn new(options: ScanOptions) -> Self {
        ScanService {
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU32::new(1),
            options,
        }
    }

    /// Starts scanning `root` (a drive like `C:` or a folder path) with `source`.
    pub fn start(&self, root: &str, source: S) -> Result<Arc<Session<S>>> {
        let trimmed = root.trim();
        if trimmed.is_empty() {
            return Err(ServiceError::BadRequest(
                "Choose a drive or folder to scan.".into(),
            ));
        }
        let root = normalize_root(Path::new(trimmed))
            .map_err(|e| ServiceError::BadRequest(format!("That location can't be used: {e}")))?;
        let handle = start(source, root.clone(), self.options.clone());
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let session = Arc::new(Session {
            id,
            root,
            cancel: handle.cancel_token(),
            state: Mutex::new(State::Running(handle)),
        });
        lock(&self.sessions).insert(id, Arc::clone(&session));
        Ok(session)
    }

    pub fn get(&self, id: u32) -> Result<Arc<Session<S>>> {
        lock(&self.sessions)
            .get(&id)
            .cloned()
            .ok_or_else(|| ServiceError::UnknownScan("That scan is no longer available.".into()))
    }

    /// Cancels (if running) and forgets a scan, freeing its memory.
    pub fn close(&self, id: u32) {
        if let Some(s) = lock(&self.sessions).remove(&id) {
            s.cancel();
        }
    }

    pub fn children(&self, id: u32, req: &ChildrenRequest) -> Result<ChildrenDto> {
        let s = self.get(id)?;
        s.with_tree(|t| {
            let node = check_node(t, req.node)?;
            let mut kids: Vec<NodeId> = t.children(node).collect();
            sort_nodes(t, &mut kids, req.sort, req.descending);
            let total = kids.len();
            let offset = (req.offset as usize).min(total);
            let end = (offset + req.limit.min(5_000) as usize).min(total);
            Ok(ChildrenDto {
                total: total as u32,
                offset: offset as u32,
                rows: kids[offset..end]
                    .iter()
                    .map(|&k| NodeRow::new(t, k))
                    .collect(),
            })
        })
    }

    /// Rows for specific nodes (treemap labels and tooltips). Unknown ids are skipped.
    pub fn rows(&self, id: u32, nodes: &[u32]) -> Result<Vec<NodeRow>> {
        if nodes.len() > 5_000 {
            return Err(ServiceError::BadRequest(
                "Too many items requested at once.".into(),
            ));
        }
        let s = self.get(id)?;
        s.with_tree(|t| {
            Ok(nodes
                .iter()
                .filter_map(|&n| check_node(t, n).ok())
                .map(|n| NodeRow::new(t, n))
                .collect())
        })
    }

    pub fn node_info(&self, id: u32, node: u32) -> Result<NodeInfoDto> {
        let s = self.get(id)?;
        s.with_tree(|t| {
            let n = check_node(t, node)?;
            let mut ancestors = Vec::new();
            let mut cur = Some(n);
            while let Some(c) = cur {
                ancestors.push(Crumb {
                    id: c.get(),
                    name: display_name(t, c),
                });
                cur = t.parent(c);
            }
            ancestors.reverse();
            Ok(NodeInfoDto {
                node: NodeRow::new(t, n),
                path: t.path(n).to_string_lossy().into_owned(),
                parent: t.parent(n).map(NodeId::get),
                ancestors,
            })
        })
    }

    /// The exact path of a node, for OS actions such as "Open in Explorer".
    pub fn path_of(&self, id: u32, node: u32) -> Result<PathBuf> {
        let s = self.get(id)?;
        s.with_tree(|t| Ok(t.path(check_node(t, node)?)))
    }

    /// Encoded treemap layout (see `clearspace_treemap::encode`).
    pub fn treemap(&self, id: u32, req: &TreemapRequest) -> Result<Vec<u8>> {
        if !(1.0..=16_384.0).contains(&req.width) || !(1.0..=16_384.0).contains(&req.height) {
            return Err(ServiceError::BadRequest(
                "Treemap size out of range.".into(),
            ));
        }
        let s = self.get(id)?;
        s.with_tree(|t| {
            let node = check_node(t, req.node)?;
            let opts = treemap::Options {
                width: req.width,
                height: req.height,
                metric: match req.metric {
                    Metric::Allocated => treemap::Metric::Allocated,
                    Metric::Logical => treemap::Metric::Logical,
                },
                max_depth: req.max_depth.clamp(1, 32),
                ..treemap::Options::default()
            };
            Ok(treemap::encode(&treemap::layout(t, node, &opts)))
        })
    }

    /// File types under `node`: per-category totals and the top extensions.
    pub fn type_breakdown(&self, id: u32, node: u32) -> Result<TypeBreakdownDto> {
        let s = self.get(id)?;
        s.with_tree(|t| {
            let root = check_node(t, node)?;
            let mut per_ext: HashMap<u16, (u64, u64, u64)> = HashMap::new();
            let mut stack = vec![root];
            while let Some(n) = stack.pop() {
                let nd = t.node(n);
                if nd.is_dir() {
                    stack.extend(t.children(n));
                } else if !nd.has(flags::LINK) && !nd.has(flags::HARDLINK_DUP) {
                    let e = per_ext.entry(nd.ext).or_default();
                    e.0 += nd.size;
                    e.1 += nd.alloc;
                    e.2 += 1;
                }
            }
            let mut categories: HashMap<Category, CategoryStat> = HashMap::new();
            let mut extensions: Vec<ExtStatDto> = per_ext
                .into_iter()
                .map(|(ext, (size, alloc, files))| {
                    let name = t.exts().name(ext).to_string();
                    let cat = category_of(&name);
                    let c = categories.entry(cat).or_insert_with(|| CategoryStat {
                        category: cat.key().into(),
                        ..Default::default()
                    });
                    c.size += size;
                    c.alloc += alloc;
                    c.files += files;
                    ExtStatDto {
                        ext: name,
                        category: cat.key().into(),
                        size,
                        alloc,
                        files,
                    }
                })
                .collect();
            extensions.sort_by(|a, b| b.alloc.cmp(&a.alloc).then_with(|| a.ext.cmp(&b.ext)));
            extensions.truncate(100);
            let mut categories: Vec<CategoryStat> = categories.into_values().collect();
            categories.sort_by(|a, b| {
                b.alloc
                    .cmp(&a.alloc)
                    .then_with(|| a.category.cmp(&b.category))
            });
            Ok(TypeBreakdownDto {
                categories,
                extensions,
            })
        })
    }
}

fn check_node(t: &Tree, raw: u32) -> Result<NodeId> {
    let id = NodeId::from_raw(raw);
    if t.contains(id) {
        Ok(id)
    } else {
        Err(ServiceError::UnknownNode(
            "That item is no longer in this scan.".into(),
        ))
    }
}

fn display_name(t: &Tree, id: NodeId) -> String {
    t.name(id).to_string_lossy().into_owned()
}

fn sort_nodes(t: &Tree, v: &mut [NodeId], key: SortKey, descending: bool) {
    match key {
        SortKey::Size => v.sort_by_key(|&n| t.node(n).size),
        SortKey::Allocated => v.sort_by_key(|&n| t.node(n).alloc),
        SortKey::Files => v.sort_by_key(|&n| t.node(n).files),
        SortKey::Modified => v.sort_by_key(|&n| t.node(n).mtime),
        SortKey::Name => {
            let mut keyed: Vec<(String, NodeId)> = v
                .iter()
                .map(|&n| (display_name(t, n).to_lowercase(), n))
                .collect();
            keyed.sort();
            for (slot, (_, n)) in v.iter_mut().zip(keyed) {
                *slot = n;
            }
        }
    }
    if descending {
        v.reverse();
    }
}

/// Status names shared with the UI.
pub fn status_key(s: &ScanStatus) -> &'static str {
    match s {
        ScanStatus::Running => "running",
        ScanStatus::Completed => "completed",
        ScanStatus::Cancelled => "cancelled",
        ScanStatus::VolumeLost => "volumeLost",
        ScanStatus::OutOfMemory(_) => "outOfMemory",
        ScanStatus::RootUnavailable(_) => "rootUnavailable",
    }
}

#[cfg(test)]
mod tests;
