//! Integration tests on real temporary directories. On Windows the same fixtures also run
//! through the native `WinSource`, including long paths, junctions, odd names, hard links
//! and NTFS compression.

use std::fs;
use std::path::Path;

use clearspace_model::{NodeId, Tree, flags};
use clearspace_scanner::scan::{ScanOptions, ScanOutcome, ScanStatus, scan_blocking};
use clearspace_scanner::source::FsSource;
use clearspace_scanner::std_source::StdSource;

fn scan<S: FsSource>(source: S, root: &Path) -> ScanOutcome {
    scan_blocking(source, root.to_path_buf(), ScanOptions { threads: 4 })
}

/// A small realistic tree; returns (root, total logical bytes, file count).
fn fixture() -> (tempfile::TempDir, u64, u32) {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join("Documents/Reports 2026")).unwrap();
    fs::create_dir_all(r.join("Pictures/Ünïcødé 日本")).unwrap();
    fs::create_dir_all(r.join("Empty")).unwrap();
    fs::write(r.join("Documents/notes.txt"), vec![b'a'; 1234]).unwrap();
    fs::write(r.join("Documents/Reports 2026/q1.pdf"), vec![0u8; 50_000]).unwrap();
    fs::write(r.join("Pictures/Ünïcødé 日本/写真 😀.jpg"), vec![1u8; 7777]).unwrap();
    fs::write(r.join("zero-byte"), b"").unwrap();
    (dir, 1234 + 50_000 + 7777, 4)
}

fn check_fixture(out: &ScanOutcome, bytes: u64, files: u32) {
    assert_eq!(out.status, ScanStatus::Completed, "{:?}", out.issues);
    assert!(out.issues.is_empty(), "{:?}", out.issues);
    let root = out.tree.node(NodeId::ROOT);
    assert_eq!(root.size, bytes);
    assert_eq!(root.files, files);
    let photo = out
        .tree
        .find(&["Pictures", "Ünïcødé 日本", "写真 😀.jpg"])
        .expect("unicode path");
    assert_eq!(out.tree.node(photo).size, 7777);
    assert_eq!(out.tree.ext_of(photo), "jpg");
    assert!(out.tree.node(out.tree.find(&["Empty"]).unwrap()).is_dir());
    let q1 = out
        .tree
        .find(&["Documents", "Reports 2026", "q1.pdf"])
        .unwrap();
    assert!(
        out.tree.node(q1).mtime > 28_000_000,
        "mtime recorded (after 2023)"
    );
    assert!(
        out.tree.node(q1).alloc >= 50_000,
        "allocation at least the size"
    );
}

trait TreeExt {
    fn ext_of(&self, id: NodeId) -> &str;
}
impl TreeExt for Tree {
    fn ext_of(&self, id: NodeId) -> &str {
        self.exts().name(self.node(id).ext)
    }
}

#[test]
fn std_source_scans_a_real_tree() {
    let (dir, bytes, files) = fixture();
    check_fixture(&scan(StdSource, dir.path()), bytes, files);
}

#[test]
fn csv_export_covers_every_node() {
    let (dir, _, _) = fixture();
    let out = scan(StdSource, dir.path());
    let mut buf = Vec::new();
    clearspace_model::csv::write_csv(&out.tree, &mut buf).unwrap();
    let text = String::from_utf8(buf).unwrap();
    assert_eq!(
        text.lines().count(),
        out.tree.len() + 1,
        "header + one row per node"
    );
    assert!(text.contains("写真 😀.jpg"));
}

#[cfg(unix)]
#[test]
fn std_source_does_not_follow_symlinks_and_dedupes_hard_links() {
    let (dir, bytes, files) = fixture();
    let r = dir.path();
    // Test fixture only: create links inside our own temp dir.
    #[allow(clippy::disallowed_methods)]
    {
        std::os::unix::fs::symlink(r.join("Documents"), r.join("docs-link")).unwrap();
        fs::hard_link(r.join("Documents/notes.txt"), r.join("notes-hardlink.txt")).unwrap();
    }
    let out = scan(StdSource, r);
    assert_eq!(
        out.tree.node(NodeId::ROOT).size,
        bytes,
        "link not followed, hard link once"
    );
    assert_eq!(out.tree.node(NodeId::ROOT).files, files + 1);
    let l = out.tree.find(&["docs-link"]).unwrap();
    assert!(out.tree.node(l).has(flags::LINK));
    assert_eq!(out.tree.children(l).len(), 0);
}

#[cfg(windows)]
mod windows {
    use super::*;
    use clearspace_scanner::win_source::WinSource;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use std::process::Command;

    fn verbatim(p: &Path) -> PathBuf {
        let s = p.as_os_str().to_string_lossy();
        if s.starts_with(r"\\?\") {
            p.to_path_buf()
        } else {
            PathBuf::from(format!(r"\\?\{s}"))
        }
    }

    #[test]
    fn win_source_matches_the_fixture() {
        let (dir, bytes, files) = fixture();
        check_fixture(&scan(WinSource::new(), dir.path()), bytes, files);
    }

    #[test]
    fn win_source_and_std_source_agree() {
        let (dir, _, _) = fixture();
        let a = scan(WinSource::new(), dir.path());
        let b = scan(StdSource, dir.path());
        let (ra, rb) = (a.tree.node(NodeId::ROOT), b.tree.node(NodeId::ROOT));
        assert_eq!((ra.size, ra.files), (rb.size, rb.files));
        assert_eq!(a.tree.len(), b.tree.len());
    }

    #[test]
    fn junctions_are_not_followed() {
        let (dir, bytes, _) = fixture();
        let r = dir.path();
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(r.join("junction"))
            .arg(r.join("Documents"))
            .status()
            .unwrap();
        assert!(status.success());
        let out = scan(WinSource::new(), r);
        assert_eq!(out.tree.node(NodeId::ROOT).size, bytes);
        let j = out.tree.find(&["junction"]).unwrap();
        assert!(out.tree.node(j).has(flags::LINK));
        assert!(out.tree.node(j).is_dir());
        assert_eq!(out.tree.children(j).len(), 0);
    }

    #[test]
    fn hard_links_count_once() {
        let (dir, bytes, files) = fixture();
        let r = dir.path();
        // Test fixture only: hard link inside our own temp dir.
        #[allow(clippy::disallowed_methods)]
        fs::hard_link(r.join("Documents/notes.txt"), r.join("notes-link.txt")).unwrap();
        let out = scan(WinSource::new(), r);
        assert_eq!(out.tree.node(NodeId::ROOT).size, bytes);
        assert_eq!(out.tree.node(NodeId::ROOT).files, files + 1);
    }

    #[test]
    fn long_paths_beyond_max_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut deep = verbatim(dir.path());
        for i in 0..12 {
            deep.push(format!("a-fairly-long-folder-name-number-{i:02}"));
        }
        assert!(deep.as_os_str().len() > 400);
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("deep.txt"), b"0123456789").unwrap();
        let out = scan(WinSource::new(), dir.path());
        assert_eq!(out.status, ScanStatus::Completed, "{:?}", out.issues);
        assert_eq!(out.tree.node(NodeId::ROOT).size, 10);
        assert_eq!(out.tree.node(NodeId::ROOT).files, 1);
    }

    #[test]
    fn names_windows_normally_rejects_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let v = verbatim(dir.path());
        fs::write(v.join("trailing dot."), b"abc").unwrap();
        fs::write(v.join("trailing space "), b"abcd").unwrap();
        let odd = OsString::from_wide(&[b'x' as u16, 0xD800, b'y' as u16]);
        fs::write(v.join(&odd), b"abcde").unwrap();
        let out = scan(WinSource::new(), dir.path());
        assert_eq!(out.tree.node(NodeId::ROOT).size, 12, "{:?}", out.issues);
        let id = out
            .tree
            .find(&[odd.as_os_str()])
            .expect("unpaired surrogate name");
        // The stored path must reopen the very same file.
        let back = fs::read(verbatim(&out.tree.path(id))).unwrap();
        assert_eq!(back, b"abcde");
        assert!(out.tree.find(&["trailing dot."]).is_some());
    }

    #[test]
    fn hidden_and_compressed_files() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        fs::write(r.join("hidden.txt"), b"h").unwrap();
        assert!(
            Command::new("attrib")
                .arg("+h")
                .arg(r.join("hidden.txt"))
                .status()
                .unwrap()
                .success()
        );
        fs::write(r.join("zeros.bin"), vec![0u8; 4 << 20]).unwrap();
        let compacted = Command::new("compact")
            .args(["/c", "/q"])
            .arg(r.join("zeros.bin"))
            .status()
            .unwrap()
            .success();
        let out = scan(WinSource::new(), r);
        let h = out.tree.find(&["hidden.txt"]).unwrap();
        assert!(out.tree.node(h).has(flags::HIDDEN));
        let z = out.tree.node(out.tree.find(&["zeros.bin"]).unwrap());
        assert_eq!(z.size, 4 << 20);
        if compacted {
            assert!(z.has(flags::COMPRESSED));
            assert!(
                z.alloc < z.size,
                "compressed allocation {} < size {}",
                z.alloc,
                z.size
            );
        }
    }

    #[test]
    fn unreadable_folder_is_reported_not_fatal() {
        let (dir, bytes, _) = fixture();
        let locked = dir.path().join("Locked");
        fs::create_dir(&locked).unwrap();
        fs::write(locked.join("inside.txt"), b"x").unwrap();
        // Deny listing to Everyone (SID *S-1-1-0) on our own temp folder.
        let ok = Command::new("icacls")
            .arg(&locked)
            .args(["/deny", "*S-1-1-0:(RD)"])
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let out = scan(WinSource::new(), dir.path());
        // Restore so the temp dir can be cleaned up.
        let _ = Command::new("icacls")
            .arg(&locked)
            .args(["/remove:d", "*S-1-1-0"])
            .status();
        assert_eq!(out.status, ScanStatus::Completed);
        assert_eq!(out.tree.node(NodeId::ROOT).size, bytes);
        assert_eq!(out.issues.len(), 1, "{:?}", out.issues);
        assert_eq!(
            out.issues[0].error.kind,
            clearspace_scanner::ListErrorKind::AccessDenied
        );
    }

    #[test]
    fn scanning_a_whole_drive_root_works() {
        // Smoke test on the system drive: must finish without crashing; issues are expected
        // (System Volume Information etc.). Bounded by cancelling after a few seconds.
        let root = clearspace_scanner::paths::normalize_root(Path::new(
            &std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()),
        ))
        .unwrap();
        let handle =
            clearspace_scanner::scan::start(WinSource::new(), root, ScanOptions::default());
        std::thread::sleep(std::time::Duration::from_secs(4));
        handle.cancel();
        let out = handle.wait();
        assert!(matches!(
            out.status,
            ScanStatus::Cancelled | ScanStatus::Completed
        ));
        assert!(
            out.progress.files > 1000,
            "only {} files",
            out.progress.files
        );
    }
}
