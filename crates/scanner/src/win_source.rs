//! Windows backend: lists each directory through one handle with
//! `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)`, which returns names, sizes,
//! allocation, attributes, reparse tags and 128-bit file ids in large batches without
//! opening any file.
//!
//! Safety-relevant behaviour:
//! * Directories are opened for listing only (`FILE_LIST_DIRECTORY`), sharing everything, so
//!   other apps are never blocked.
//! * Name-surrogate reparse points (junctions, symlinks, mount points) are reported as
//!   links and never followed. Before listing a directory we check its file id still matches
//!   the id seen in the parent's listing, so a folder swapped for a junction mid-scan is
//!   skipped rather than followed.
//! * Cloud placeholders (OneDrive Files On-Demand, …) are flagged from their attributes and
//!   never opened, so scanning never downloads ("hydrates") them.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use clearspace_model::{flags, time::minutes_from_filetime};
use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FULL_DIR_INFO, FILE_ID_EXTD_DIR_INFO,
    FILE_ID_INFO, FILE_INFO_BY_HANDLE_CLASS, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileFullDirectoryInfo,
    FileFullDirectoryRestartInfo, FileIdExtdDirectoryInfo, FileIdExtdDirectoryRestartInfo,
    FileIdInfo, GetCompressedFileSizeW, GetFileInformationByHandleEx, INVALID_FILE_SIZE,
    OPEN_EXISTING, SYNCHRONIZE,
};
use windows::core::PCWSTR;

use crate::paths::verbatim_wide;
use crate::source::{
    EntryKind, FsSource, ListError, ListErrorKind, Listing, RawEntry, kind_from_os_code,
};

const ATTR_READONLY: u32 = 0x1;
const ATTR_HIDDEN: u32 = 0x2;
const ATTR_SYSTEM: u32 = 0x4;
const ATTR_DIRECTORY: u32 = 0x10;
const ATTR_SPARSE: u32 = 0x200;
const ATTR_REPARSE: u32 = 0x400;
const ATTR_COMPRESSED: u32 = 0x800;
const ATTR_OFFLINE: u32 = 0x1000;
const ATTR_RECALL_ON_OPEN: u32 = 0x40000;
const ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x400000;
/// `IsReparseTagNameSurrogate`: the reparse point stands for another named entity (junction,
/// symlink, mount point). Cloud-file and dedup tags are *not* surrogates: those are real
/// files/folders and must be scanned (without hydrating).
const TAG_NAME_SURROGATE_BIT: u32 = 0x2000_0000;
/// Windows Overlay Filter (CompactOS / `compact /exe`) compressed file.
const IO_REPARSE_TAG_WOF: u32 = 0x8000_0017;

const ERROR_NO_MORE_FILES: i32 = 18;
const ERROR_INVALID_PARAMETER: i32 = 87;
const ERROR_NOT_SUPPORTED: i32 = 50;

/// The Windows directory-enumeration backend.
#[derive(Debug, Default)]
pub struct WinSource {
    /// Set once a volume rejects the id-extended class (FAT/exFAT, some network shares);
    /// later listings go straight to the full-directory class.
    no_extended: AtomicBool,
}

impl WinSource {
    pub fn new() -> WinSource {
        WinSource::default()
    }
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful CreateFileW and is closed exactly once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn wide_nul(path: &Path) -> Vec<u16> {
    let mut w = verbatim_wide(&path.as_os_str().encode_wide().collect::<Vec<_>>());
    w.push(0);
    w
}

fn win32_code(e: &windows::core::Error) -> i32 {
    let hr = e.code().0 as u32;
    if hr & 0xFFFF_0000 == 0x8007_0000 {
        (hr & 0xFFFF) as i32
    } else {
        hr as i32
    }
}

fn list_error(e: &windows::core::Error) -> ListError {
    let code = win32_code(e);
    ListError {
        kind: kind_from_os_code(code),
        code: Some(code),
    }
}

fn open_dir(path: &Path) -> Result<Handle, ListError> {
    let wide = wide_nul(path);
    // Opened *without* FILE_FLAG_OPEN_REPARSE_POINT so that cloud-placeholder folders are
    // listed through their sync provider. Junction-following is prevented by the file-id
    // check in `list`, not by this flag.
    // SAFETY: `wide` is a NUL-terminated UTF-16 buffer that outlives the call.
    let h = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0 | SYNCHRONIZE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
    }
    .map_err(|e| list_error(&e))?;
    Ok(Handle(h))
}

fn file_id(h: &Handle) -> Result<u128, ListError> {
    let mut info = FILE_ID_INFO::default();
    // SAFETY: `info` is a properly sized, writable FILE_ID_INFO.
    unsafe {
        GetFileInformationByHandleEx(
            h.0,
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    }
    .map_err(|e| list_error(&e))?;
    Ok(u128::from_le_bytes(info.FileId.Identifier))
}

/// Allocated size of a compressed / sparse / WOF file, which the directory entry does not
/// report accurately. Opens no data stream. Never called for cloud placeholders.
fn compressed_size(dir: &Path, name: &OsString) -> Option<u64> {
    let wide = wide_nul(&dir.join(name));
    let mut high = 0u32;
    // SAFETY: NUL-terminated path buffer and a valid out-pointer.
    let low = unsafe { GetCompressedFileSizeW(PCWSTR(wide.as_ptr()), Some(&mut high)) };
    // SAFETY: GetLastError has no preconditions.
    if low == INVALID_FILE_SIZE && unsafe { GetLastError() }.0 != 0 {
        return None;
    }
    Some((u64::from(high) << 32) | u64::from(low))
}

/// Fields shared by both directory-information classes.
struct DirRecord {
    name: OsString,
    attrs: u32,
    tag: u32,
    size: i64,
    alloc: i64,
    mtime: i64,
    id: Option<u128>,
}

/// Walks a buffer of variable-length directory records.
///
/// # Safety
/// `buf` must hold records of class `extended ? FILE_ID_EXTD_DIR_INFO : FILE_FULL_DIR_INFO`
/// as written by a successful `GetFileInformationByHandleEx` call.
unsafe fn parse_records(buf: &[u64], extended: bool, out: &mut Vec<DirRecord>) {
    let base = buf.as_ptr().cast::<u8>();
    let len = std::mem::size_of_val(buf);
    let mut off = 0usize;
    loop {
        if off >= len {
            break;
        }
        // SAFETY (whole block): offsets come from the OS-written NextEntryOffset chain inside
        // `buf`; every field is read unaligned through raw pointers; names are bounded by
        // FileNameLength and checked against the buffer end.
        let (next, rec) = unsafe {
            let p = base.add(off);
            if extended {
                let r = p.cast::<FILE_ID_EXTD_DIR_INFO>();
                let name_len = std::ptr::addr_of!((*r).FileNameLength).read_unaligned() as usize;
                let name_ptr = std::ptr::addr_of!((*r).FileName).cast::<u8>();
                if name_ptr.offset_from(base) as usize + name_len > len {
                    break;
                }
                let name = read_name(name_ptr, name_len);
                (
                    std::ptr::addr_of!((*r).NextEntryOffset).read_unaligned(),
                    DirRecord {
                        name,
                        attrs: std::ptr::addr_of!((*r).FileAttributes).read_unaligned(),
                        tag: std::ptr::addr_of!((*r).ReparsePointTag).read_unaligned(),
                        size: std::ptr::addr_of!((*r).EndOfFile).read_unaligned(),
                        alloc: std::ptr::addr_of!((*r).AllocationSize).read_unaligned(),
                        mtime: std::ptr::addr_of!((*r).LastWriteTime).read_unaligned(),
                        id: Some(u128::from_le_bytes(
                            std::ptr::addr_of!((*r).FileId).read_unaligned().Identifier,
                        )),
                    },
                )
            } else {
                let r = p.cast::<FILE_FULL_DIR_INFO>();
                let name_len = std::ptr::addr_of!((*r).FileNameLength).read_unaligned() as usize;
                let name_ptr = std::ptr::addr_of!((*r).FileName).cast::<u8>();
                if name_ptr.offset_from(base) as usize + name_len > len {
                    break;
                }
                let name = read_name(name_ptr, name_len);
                let attrs = std::ptr::addr_of!((*r).FileAttributes).read_unaligned();
                // For this class, EaSize holds the reparse tag when the entry is a reparse
                // point (FILE_FULL_DIR_INFORMATION documentation).
                let ea = std::ptr::addr_of!((*r).EaSize).read_unaligned();
                (
                    std::ptr::addr_of!((*r).NextEntryOffset).read_unaligned(),
                    DirRecord {
                        name,
                        attrs,
                        tag: if attrs & ATTR_REPARSE != 0 { ea } else { 0 },
                        size: std::ptr::addr_of!((*r).EndOfFile).read_unaligned(),
                        alloc: std::ptr::addr_of!((*r).AllocationSize).read_unaligned(),
                        mtime: std::ptr::addr_of!((*r).LastWriteTime).read_unaligned(),
                        id: None,
                    },
                )
            }
        };
        out.push(rec);
        if next == 0 {
            break;
        }
        off += next as usize;
    }
}

/// Copies a possibly unaligned UTF-16 name out of the buffer.
///
/// # Safety
/// `ptr..ptr+len_bytes` must be readable.
unsafe fn read_name(ptr: *const u8, len_bytes: usize) -> OsString {
    let mut units = Vec::with_capacity(len_bytes / 2);
    for i in 0..len_bytes / 2 {
        // SAFETY: in bounds per the caller's contract.
        units.push(unsafe { ptr.add(i * 2).cast::<u16>().read_unaligned() });
    }
    OsString::from_wide(&units)
}

impl WinSource {
    fn read_all(&self, h: &Handle, dir: &Path) -> Result<Listing, ListError> {
        let mut extended = !self.no_extended.load(Ordering::Relaxed);
        let mut buf = vec![0u64; 64 * 1024 / 8];
        let mut records = Vec::new();
        let mut first = true;
        let mut listing = Listing::default();
        loop {
            let class: FILE_INFO_BY_HANDLE_CLASS = match (extended, first) {
                (true, true) => FileIdExtdDirectoryRestartInfo,
                (true, false) => FileIdExtdDirectoryInfo,
                (false, true) => FileFullDirectoryRestartInfo,
                (false, false) => FileFullDirectoryInfo,
            };
            // SAFETY: `buf` is writable for its full byte length.
            let r = unsafe {
                GetFileInformationByHandleEx(
                    h.0,
                    class,
                    buf.as_mut_ptr().cast(),
                    std::mem::size_of_val(buf.as_slice()) as u32,
                )
            };
            match r {
                Ok(()) => {
                    // SAFETY: the call succeeded with this class into this buffer.
                    unsafe { parse_records(&buf, extended, &mut records) };
                    first = false;
                }
                Err(e) => {
                    let code = win32_code(&e);
                    if code == ERROR_NO_MORE_FILES {
                        break;
                    }
                    if first
                        && extended
                        && (code == ERROR_INVALID_PARAMETER || code == ERROR_NOT_SUPPORTED)
                    {
                        self.no_extended.store(true, Ordering::Relaxed);
                        extended = false;
                        continue;
                    }
                    if first {
                        return Err(list_error(&e));
                    }
                    listing.partial = Some(list_error(&e));
                    break;
                }
            }
        }
        listing.entries.reserve(records.len());
        for r in records {
            if r.name == "." || r.name == ".." {
                continue;
            }
            listing.entries.push(to_entry(dir, r));
        }
        Ok(listing)
    }
}

fn to_entry(dir: &Path, r: DirRecord) -> RawEntry {
    let is_dir = r.attrs & ATTR_DIRECTORY != 0;
    let is_link = r.attrs & ATTR_REPARSE != 0 && r.tag & TAG_NAME_SURROGATE_BIT != 0;
    let kind = match (is_link, is_dir) {
        (true, d) => EntryKind::Link { dir: d },
        (false, true) => EntryKind::Dir,
        (false, false) => EntryKind::File,
    };
    let cloud = r.attrs & (ATTR_RECALL_ON_DATA_ACCESS | ATTR_RECALL_ON_OPEN | ATTR_OFFLINE) != 0;
    let mut f = 0u16;
    for (attr, flag) in [
        (ATTR_READONLY, flags::READONLY),
        (ATTR_HIDDEN, flags::HIDDEN),
        (ATTR_SYSTEM, flags::SYSTEM),
        (ATTR_COMPRESSED, flags::COMPRESSED),
        (ATTR_SPARSE, flags::SPARSE),
    ] {
        if r.attrs & attr != 0 {
            f |= flag;
        }
    }
    if cloud {
        f |= flags::CLOUD;
    }
    let size = r.size.max(0) as u64;
    let mut alloc = r.alloc.max(0) as u64;
    let needs_precise_alloc = kind == EntryKind::File
        && !cloud
        && (r.attrs & (ATTR_COMPRESSED | ATTR_SPARSE) != 0
            || (r.attrs & ATTR_REPARSE != 0 && r.tag == IO_REPARSE_TAG_WOF));
    if needs_precise_alloc && let Some(a) = compressed_size(dir, &r.name) {
        alloc = a;
    }
    RawEntry {
        name: r.name,
        kind,
        size: if kind == EntryKind::File { size } else { 0 },
        alloc: if kind == EntryKind::File { alloc } else { 0 },
        mtime: minutes_from_filetime(r.mtime),
        flags: f,
        file_id: r.id,
        // Directory listings carry no link count, so every file with a known id is a
        // candidate; the engine counts each id once.
        maybe_hardlink: kind == EntryKind::File && r.id.is_some(),
    }
}

impl FsSource for WinSource {
    fn open_root(&self, root: &Path) -> Result<Option<u128>, ListError> {
        let h = open_dir(root)?;
        Ok(file_id(&h).ok())
    }

    fn list(&self, dir: &Path, expected_id: Option<u128>) -> Result<Listing, ListError> {
        let h = open_dir(dir)?;
        if let Some(expected) = expected_id {
            // Only enforce when the volume reports ids; FAT reports none / unstable ones.
            if let Ok(actual) = file_id(&h)
                && actual != expected
                && !self.no_extended.load(Ordering::Relaxed)
            {
                return Err(ListError::new(ListErrorKind::Changed));
            }
        }
        self.read_all(&h, dir)
    }
}
