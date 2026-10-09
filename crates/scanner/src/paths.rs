//! Path helpers for scan roots and Win32 long-path ("verbatim") form.

use std::io;
use std::path::{Path, PathBuf};

/// Turns user input into an absolute display path for a scan root: `C:` becomes `C:\`,
/// relative paths are resolved against the current directory and `..` is collapsed.
pub fn normalize_root(input: &Path) -> io::Result<PathBuf> {
    let s = input.as_os_str().as_encoded_bytes();
    if s.len() == 2 && s[0].is_ascii_alphabetic() && s[1] == b':' {
        let mut p = input.as_os_str().to_owned();
        p.push("\\");
        return Ok(PathBuf::from(p));
    }
    std::path::absolute(input)
}

/// Converts an absolute UTF-16 path to the `\\?\` verbatim form Win32 needs for paths longer
/// than `MAX_PATH` and for names with trailing dots/spaces. Forward slashes are converted to
/// backslashes (verbatim paths do not accept `/`). Already-verbatim and device paths
/// (`\\?\`, `\\.\`) are returned unchanged.
///
/// Callers must pass an absolute, already-normalised path ([`normalize_root`]); verbatim
/// paths bypass Windows' own `.`/`..` processing.
pub fn verbatim_wide(path: &[u16]) -> Vec<u16> {
    const BS: u16 = b'\\' as u16;
    let p: Vec<u16> = path
        .iter()
        .map(|&c| if c == b'/' as u16 { BS } else { c })
        .collect();
    let starts = |prefix: &str| {
        let pre: Vec<u16> = prefix.encode_utf16().collect();
        p.starts_with(&pre)
    };
    if starts(r"\\?\") || starts(r"\\.\") || starts(r"\??\") {
        return p;
    }
    let mut out: Vec<u16> = Vec::with_capacity(p.len() + 8);
    if starts(r"\\") {
        out.extend(r"\\?\UNC\".encode_utf16());
        out.extend_from_slice(&p[2..]);
    } else {
        out.extend(r"\\?\".encode_utf16());
        out.extend_from_slice(&p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> String {
        String::from_utf16(&verbatim_wide(&s.encode_utf16().collect::<Vec<_>>())).unwrap()
    }

    #[test]
    fn verbatim_forms() {
        assert_eq!(v(r"C:\Users"), r"\\?\C:\Users");
        assert_eq!(v("C:/Users/x"), r"\\?\C:\Users\x");
        assert_eq!(v(r"\\server\share\dir"), r"\\?\UNC\server\share\dir");
        assert_eq!(v(r"\\?\C:\already"), r"\\?\C:\already");
        assert_eq!(v(r"\\.\PhysicalDrive0"), r"\\.\PhysicalDrive0");
        assert_eq!(v(r"C:\trailing dot."), r"\\?\C:\trailing dot.");
    }

    #[test]
    fn drive_letter_root() {
        assert_eq!(
            normalize_root(Path::new("C:")).unwrap(),
            PathBuf::from("C:\\")
        );
        assert!(normalize_root(Path::new("relative")).unwrap().is_absolute());
    }
}
