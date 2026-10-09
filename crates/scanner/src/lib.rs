//! ClearSpace's directory scanner: the no-admin path that works on any volume.
//!
//! ```no_run
//! use clearspace_scanner::{default_source, paths::normalize_root, scan::{start, ScanOptions}};
//! let root = normalize_root("C:\\".as_ref()).unwrap();
//! let handle = start(default_source(), root, ScanOptions::default());
//! println!("{:?}", handle.progress());
//! let outcome = handle.wait();
//! println!("{} entries, status {:?}", outcome.tree.len(), outcome.status);
//! ```
//!
//! This crate only *reads*. It never opens file contents (so cloud placeholders are never
//! downloaded) and never follows junctions or symbolic links.

pub mod fake;
pub mod paths;
pub mod scan;
pub mod source;
pub mod std_source;
#[cfg(windows)]
pub mod win_source;

pub use scan::{CancelToken, Issue, Progress, ScanHandle, ScanOptions, ScanOutcome, ScanStatus};
pub use source::{FsSource, ListError, ListErrorKind};

/// The best directory source for this platform.
#[cfg(windows)]
pub type DefaultSource = win_source::WinSource;
/// The best directory source for this platform.
#[cfg(not(windows))]
pub type DefaultSource = std_source::StdSource;

pub fn default_source() -> DefaultSource {
    DefaultSource::default()
}
