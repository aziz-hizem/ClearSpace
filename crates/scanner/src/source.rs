//! The file-system abstraction the scan engine runs on.
//!
//! The engine never touches the OS directly; it asks an [`FsSource`] to list one directory at
//! a time. That keeps the engine identical across the Windows backend, the portable `std`
//! backend and the in-memory fake used by tests (drives vanishing, access denied, …).
//!
//! Every source is **read-only by construction**: the trait has no method that could write.

use std::ffi::OsString;
use std::fmt;
use std::path::Path;

/// What a directory entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    /// A symlink, junction or volume mount point (a "name surrogate" reparse point). It is
    /// recorded but never followed; `dir` says whether it points at a directory.
    Link {
        dir: bool,
    },
}

/// One entry of a directory listing.
#[derive(Clone, Debug)]
pub struct RawEntry {
    pub name: OsString,
    pub kind: EntryKind,
    /// Logical size in bytes.
    pub size: u64,
    /// Allocated bytes on disk.
    pub alloc: u64,
    /// Last write, in stored minutes (see `clearspace_model::time`).
    pub mtime: u32,
    /// Extra `clearspace_model::flags` (CLOUD, HIDDEN, …). DIR / LINK are derived from `kind`.
    pub flags: u16,
    /// Stable file identity on its volume, if the source knows it.
    pub file_id: Option<u128>,
    /// True if this file may have other hard links (link count > 1 or unknown).
    pub maybe_hardlink: bool,
}

/// Why a directory could not be (fully) listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ListErrorKind {
    /// No permission to list this folder.
    AccessDenied,
    /// The folder disappeared after it was discovered.
    NotFound,
    /// The folder was replaced (e.g. by a junction) after it was discovered; not followed.
    Changed,
    /// The drive was disconnected or became unreadable. Ends the scan.
    VolumeGone,
    /// A cloud-sync provider (OneDrive, …) is not available for this placeholder folder.
    CloudUnavailable,
    Other,
}

/// A listing failure with the OS error code (if any) for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListError {
    pub kind: ListErrorKind,
    pub code: Option<i32>,
}

impl ListError {
    pub fn new(kind: ListErrorKind) -> ListError {
        ListError { kind, code: None }
    }

    /// Maps a `std::io::Error` (used by the portable source).
    pub fn from_io(e: &std::io::Error) -> ListError {
        use std::io::ErrorKind as K;
        let kind = match e.kind() {
            K::PermissionDenied => ListErrorKind::AccessDenied,
            K::NotFound => ListErrorKind::NotFound,
            _ => match e.raw_os_error() {
                Some(code) => kind_from_os_code(code),
                None => ListErrorKind::Other,
            },
        };
        ListError {
            kind,
            code: e.raw_os_error(),
        }
    }

    /// Plain-language description for the UI and logs.
    pub fn describe(&self) -> &'static str {
        match self.kind {
            ListErrorKind::AccessDenied => "Windows doesn't allow this folder to be read",
            ListErrorKind::NotFound => "The folder was removed while scanning",
            ListErrorKind::Changed => "The folder changed while scanning and was skipped",
            ListErrorKind::VolumeGone => "The drive was disconnected or stopped responding",
            ListErrorKind::CloudUnavailable => "The cloud-sync app for this folder isn't running",
            ListErrorKind::Other => "The folder couldn't be read",
        }
    }
}

impl fmt::Display for ListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            Some(c) => write!(f, "{} (error {c})", self.describe()),
            None => f.write_str(self.describe()),
        }
    }
}

impl std::error::Error for ListError {}

/// Classifies a raw OS error code. Windows codes on Windows, errno elsewhere.
pub fn kind_from_os_code(code: i32) -> ListErrorKind {
    #[cfg(windows)]
    {
        match code {
            5 | 1920 => ListErrorKind::AccessDenied,
            2 | 3 | 123 | 267 => ListErrorKind::NotFound,
            // NOT_READY, INVALID_DRIVE, DEV_NOT_EXIST, NETNAME_DELETED, UNRECOGNIZED_VOLUME,
            // FILE_INVALID (volume externally altered), DEVICE_NOT_CONNECTED, MEDIA_CHANGED
            21 | 15 | 55 | 64 | 1005 | 1006 | 1167 | 1110 => ListErrorKind::VolumeGone,
            // ERROR_CLOUD_FILE_* range
            358..=406 => ListErrorKind::CloudUnavailable,
            _ => ListErrorKind::Other,
        }
    }
    #[cfg(not(windows))]
    {
        match code {
            1 | 13 => ListErrorKind::AccessDenied,         // EPERM, EACCES
            2 | 20 => ListErrorKind::NotFound,             // ENOENT, ENOTDIR
            5 | 6 | 19 | 107 => ListErrorKind::VolumeGone, // EIO, ENXIO, ENODEV, ENOTCONN
            _ => ListErrorKind::Other,
        }
    }
}

/// A directory listing. `partial` is set when enumeration failed part-way: the entries
/// gathered so far are kept and the folder is flagged as incomplete.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub entries: Vec<RawEntry>,
    pub partial: Option<ListError>,
}

/// A read-only view of a file system that can list directories.
pub trait FsSource: Send + Sync + 'static {
    /// Checks that `root` is a readable directory and returns its file id (if known).
    fn open_root(&self, root: &Path) -> Result<Option<u128>, ListError>;

    /// Lists `dir`. When `expected_id` is given and the directory now has a different
    /// identity (e.g. it was swapped for a junction), returns [`ListErrorKind::Changed`]
    /// instead of following it.
    fn list(&self, dir: &Path, expected_id: Option<u128>) -> Result<Listing, ListError>;
}
