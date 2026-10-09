//! Lists drives for the Overview screen. Read-only.

use clearspace_service::DriveDto;

#[cfg(windows)]
pub fn list() -> Vec<DriveDto> {
    use std::sync::mpsc;
    use std::time::Duration;

    let roots = logical_drive_roots();
    // Network or sleeping drives can take seconds to answer; query each on its own thread
    // and give up (showing the drive as "not ready") rather than freezing the Overview.
    let (tx, rx) = mpsc::channel();
    for (i, root) in roots.iter().enumerate() {
        let tx = tx.clone();
        let root = root.clone();
        std::thread::spawn(move || {
            let _ = tx.send((i, query(&root)));
        });
    }
    drop(tx);
    let mut out: Vec<Option<DriveDto>> = vec![None; roots.len()];
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while let Ok((i, d)) =
        rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
    {
        out[i] = Some(d);
    }
    roots
        .into_iter()
        .zip(out)
        .map(|(root, d)| d.unwrap_or_else(|| not_ready(&root, "unknown")))
        .collect()
}

#[cfg(windows)]
fn logical_drive_roots() -> Vec<String> {
    use windows::Win32::Storage::FileSystem::GetLogicalDriveStringsW;
    let mut buf = vec![0u16; 512];
    // SAFETY: the buffer is valid for its length; the call writes a double-NUL list.
    let n = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) } as usize;
    if n == 0 || n > buf.len() {
        return Vec::new();
    }
    buf[..n]
        .split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

#[cfg(windows)]
fn not_ready(root: &str, kind: &'static str) -> DriveDto {
    DriveDto {
        root: root.to_string(),
        label: String::new(),
        file_system: String::new(),
        kind,
        total_bytes: 0,
        free_bytes: 0,
        ready: false,
    }
}

#[cfg(windows)]
fn query(root: &str) -> DriveDto {
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetVolumeInformationW,
    };
    use windows::core::PCWSTR;

    let w = wide(root);
    // SAFETY: NUL-terminated root path that outlives the calls; buffers sized by slices.
    let kind = match unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) } {
        2 => "removable",
        3 => "fixed",
        4 => "network",
        5 => "optical",
        6 => "ram",
        _ => "unknown",
    };
    let mut label = [0u16; 261];
    let mut fs = [0u16; 261];
    let info = unsafe {
        GetVolumeInformationW(
            PCWSTR(w.as_ptr()),
            Some(&mut label),
            None,
            None,
            None,
            Some(&mut fs),
        )
    };
    if info.is_err() {
        return not_ready(root, kind);
    }
    let (mut free, mut total) = (0u64, 0u64);
    let space =
        unsafe { GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), Some(&mut free), Some(&mut total), None) };
    let text = |b: &[u16]| {
        String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())])
    };
    DriveDto {
        root: root.to_string(),
        label: text(&label),
        file_system: text(&fs),
        kind,
        total_bytes: total,
        free_bytes: free,
        ready: space.is_ok(),
    }
}

/// Non-Windows builds (development only) report no drives.
#[cfg(not(windows))]
pub fn list() -> Vec<DriveDto> {
    Vec::new()
}
