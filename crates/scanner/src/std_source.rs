//! Portable backend on `std::fs`. Used on non-Windows hosts (development, tests) and as a
//! reference implementation. Never follows symlinks.

use std::path::Path;

use clearspace_model::time::minutes_from_system_time;

use crate::source::{EntryKind, FsSource, ListError, ListErrorKind, Listing, RawEntry};

/// Lists directories with `std::fs::read_dir` + `symlink_metadata`.
#[derive(Clone, Copy, Debug, Default)]
pub struct StdSource;

#[cfg(unix)]
fn identity(md: &std::fs::Metadata) -> (Option<u128>, bool, u64) {
    use std::os::unix::fs::MetadataExt;
    let id = (u128::from(md.dev()) << 64) | u128::from(md.ino());
    (Some(id), md.nlink() > 1, md.blocks() * 512)
}

#[cfg(not(unix))]
fn identity(md: &std::fs::Metadata) -> (Option<u128>, bool, u64) {
    // std exposes no stable file id or allocation size on Windows; the Windows backend
    // (`WinSource`) is the real implementation there.
    (None, false, md.len())
}

impl FsSource for StdSource {
    fn open_root(&self, root: &Path) -> Result<Option<u128>, ListError> {
        let md = std::fs::metadata(root).map_err(|e| ListError::from_io(&e))?;
        if !md.is_dir() {
            return Err(ListError::new(ListErrorKind::NotFound));
        }
        Ok(identity(&md).0)
    }

    fn list(&self, dir: &Path, expected_id: Option<u128>) -> Result<Listing, ListError> {
        let md = std::fs::symlink_metadata(dir).map_err(|e| ListError::from_io(&e))?;
        // The engine only lists the root and plain directories. A link here is either a
        // root the user chose (followed once, deliberately) or a directory swapped for a
        // link mid-scan, which the identity check below rejects.
        let md = if md.file_type().is_symlink() {
            std::fs::metadata(dir).map_err(|e| ListError::from_io(&e))?
        } else {
            md
        };
        if !md.is_dir() || (expected_id.is_some() && identity(&md).0 != expected_id) {
            return Err(ListError::new(ListErrorKind::Changed));
        }
        let rd = std::fs::read_dir(dir).map_err(|e| ListError::from_io(&e))?;
        let mut listing = Listing::default();
        for item in rd {
            let item = match item {
                Ok(i) => i,
                Err(e) => {
                    listing.partial = Some(ListError::from_io(&e));
                    break;
                }
            };
            let md = match item.metadata() {
                Ok(m) => m,
                // Vanished between readdir and stat: skip silently, like the OS would have.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    listing.partial = Some(ListError::from_io(&e));
                    continue;
                }
            };
            let ft = md.file_type();
            let (file_id, maybe_hardlink, alloc) = identity(&md);
            let kind = if ft.is_symlink() {
                let target_is_dir = std::fs::metadata(item.path()).is_ok_and(|m| m.is_dir());
                EntryKind::Link { dir: target_is_dir }
            } else if ft.is_dir() {
                EntryKind::Dir
            } else {
                EntryKind::File
            };
            let mut flags = 0;
            if md.permissions().readonly() {
                flags |= clearspace_model::flags::READONLY;
            }
            listing.entries.push(RawEntry {
                name: item.file_name(),
                kind,
                size: if kind == EntryKind::File { md.len() } else { 0 },
                alloc: if kind == EntryKind::File { alloc } else { 0 },
                mtime: md.modified().map_or(0, minutes_from_system_time),
                flags,
                file_id,
                maybe_hardlink: kind == EntryKind::File && maybe_hardlink,
            });
        }
        Ok(listing)
    }
}
