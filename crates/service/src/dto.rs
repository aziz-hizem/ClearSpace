//! Data shapes exchanged with the UI (serialised as camelCase JSON). Mirrored by hand in
//! `ui/src/lib/types.ts`; `crates/service/src/tests.rs` pins the JSON shape.

use std::path::Path;

use clearspace_model::filetype::category_of;
use clearspace_model::{NodeId, Tree, flags};
use clearspace_scanner::scan::{Issue, Progress, ScanStatus};
use serde::{Deserialize, Serialize};

use crate::status_key;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressDto {
    pub scan_id: u32,
    pub root: String,
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub alloc: u64,
    pub issues: u64,
    pub elapsed_ms: u64,
    pub current: Option<String>,
    /// `running | completed | cancelled | volumeLost | outOfMemory | rootUnavailable`
    pub status: &'static str,
    /// Plain-language detail for failure states.
    pub message: Option<String>,
}

impl ProgressDto {
    pub fn from_progress(scan_id: u32, root: &Path, p: &Progress) -> Self {
        let message = match &p.status {
            ScanStatus::RootUnavailable(e) => Some(e.to_string()),
            ScanStatus::OutOfMemory(e) => Some(e.to_string()),
            ScanStatus::VolumeLost => {
                Some("The drive was disconnected or stopped responding.".into())
            }
            _ => None,
        };
        ProgressDto {
            scan_id,
            root: root.to_string_lossy().into_owned(),
            files: p.files,
            dirs: p.dirs,
            bytes: p.bytes,
            alloc: p.alloc,
            issues: p.issues,
            elapsed_ms: p.elapsed.as_millis() as u64,
            current: p.current.as_ref().map(|c| c.to_string_lossy().into_owned()),
            status: status_key(&p.status),
            message,
        }
    }
}

/// One row of the tree/list view.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeRow {
    pub id: u32,
    pub name: String,
    pub is_dir: bool,
    /// A junction/symlink: shown, never followed, counts zero bytes.
    pub is_link: bool,
    pub size: u64,
    pub alloc: u64,
    /// Files beneath (folders only).
    pub files: u32,
    /// Minutes since the Unix epoch (0 = unknown).
    pub mtime: u32,
    pub child_count: u32,
    /// Cloud-only placeholder (OneDrive etc.).
    pub cloud: bool,
    pub hidden: bool,
    /// The folder could not be (fully) read.
    pub unreadable: bool,
    /// Another name for a file already counted elsewhere.
    pub hardlink: bool,
    /// File category key (files only).
    pub category: Option<&'static str>,
}

impl NodeRow {
    pub fn new(t: &Tree, id: NodeId) -> Self {
        let n = t.node(id);
        NodeRow {
            id: id.get(),
            name: t.name(id).to_string_lossy().into_owned(),
            is_dir: n.is_dir(),
            is_link: n.has(flags::LINK),
            size: n.size,
            alloc: n.alloc,
            files: n.files,
            mtime: n.mtime,
            child_count: n.child_count(),
            cloud: n.has(flags::CLOUD),
            hidden: n.has(flags::HIDDEN) || n.has(flags::SYSTEM),
            unreadable: n.has(flags::UNREADABLE),
            hardlink: n.has(flags::HARDLINK_DUP),
            category: (!n.is_dir()).then(|| category_of(t.exts().name(n.ext)).key()),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
    Size,
    Allocated,
    Name,
    Files,
    Modified,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChildrenRequest {
    pub node: u32,
    pub sort: SortKey,
    pub descending: bool,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChildrenDto {
    pub total: u32,
    pub offset: u32,
    pub rows: Vec<NodeRow>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Crumb {
    pub id: u32,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfoDto {
    pub node: NodeRow,
    pub path: String,
    pub parent: Option<u32>,
    /// Root first, ending with the node itself.
    pub ancestors: Vec<Crumb>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Metric {
    Allocated,
    Logical,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TreemapRequest {
    pub node: u32,
    pub width: f32,
    pub height: f32,
    pub metric: Metric,
    pub max_depth: u8,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CategoryStat {
    pub category: String,
    pub size: u64,
    pub alloc: u64,
    pub files: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtStatDto {
    pub ext: String,
    pub category: String,
    pub size: u64,
    pub alloc: u64,
    pub files: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TypeBreakdownDto {
    pub categories: Vec<CategoryStat>,
    pub extensions: Vec<ExtStatDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IssueDto {
    pub node: u32,
    pub path: String,
    pub reason: String,
    pub partial: bool,
}

impl From<&Issue> for IssueDto {
    fn from(i: &Issue) -> Self {
        IssueDto {
            node: i.node.get(),
            path: i.path.to_string_lossy().into_owned(),
            reason: i.error.describe().to_string(),
            partial: i.partial,
        }
    }
}

/// A drive shown on the Overview screen.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DriveDto {
    /// e.g. `C:\`
    pub root: String,
    pub label: String,
    pub file_system: String,
    /// `fixed | removable | network | optical | ram | unknown`
    pub kind: &'static str,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// False for card readers without media and similar.
    pub ready: bool,
}
