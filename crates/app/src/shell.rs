//! "Open in Explorer": shows an item in File Explorer with it selected. This only opens a
//! window; it never changes the item.

use std::path::Path;

#[cfg(windows)]
pub fn reveal(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};
    use windows::core::PCWSTR;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // The shell needs COM on the calling thread; use a short-lived thread of our own so the
    // async runtime's threads are never left in an unexpected apartment.
    std::thread::spawn(move || {
        // SAFETY: COM is initialised and uninitialised on this thread only; the PIDL is freed
        // exactly once; `wide` is NUL-terminated and outlives the calls.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let pidl = ILCreateFromPathW(PCWSTR(wide.as_ptr()));
            let result = if pidl.is_null() {
                Err(
                    "Windows couldn't find this item. It may have been moved or removed."
                        .to_string(),
                )
            } else {
                let r = SHOpenFolderAndSelectItems(pidl, None, 0)
                    .map_err(|e| format!("File Explorer couldn't open this location ({e})."));
                ILFree(Some(pidl));
                r
            };
            CoUninitialize();
            result
        }
    })
    .join()
    .map_err(|_| "File Explorer couldn't be opened.".to_string())?
}

#[cfg(not(windows))]
pub fn reveal(path: &Path) -> Result<(), String> {
    Err(format!(
        "Opening {} is only supported on Windows.",
        path.display()
    ))
}
