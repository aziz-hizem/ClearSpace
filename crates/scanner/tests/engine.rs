//! Scan-engine behaviour on the in-memory fake file system.

use std::path::PathBuf;
use std::time::Duration;

use clearspace_model::{NodeId, flags};
use clearspace_scanner::fake::{FakeFs, wide_tree};
use clearspace_scanner::scan::{ScanOptions, ScanStatus, scan_blocking, start};
use clearspace_scanner::source::ListErrorKind;

fn opts(threads: usize) -> ScanOptions {
    ScanOptions { threads }
}

fn root() -> PathBuf {
    PathBuf::from("R")
}

#[test]
fn totals_and_structure() {
    let fs = FakeFs::new(root())
        .file("a.txt", 100)
        .file("docs/b.pdf", 2000)
        .file("docs/deep/c.bin", 30_000)
        .dir("empty");
    let out = scan_blocking(fs, root(), opts(4));
    assert_eq!(out.status, ScanStatus::Completed);
    assert!(out.issues.is_empty(), "{:?}", out.issues);
    let t = &out.tree;
    assert_eq!(t.node(NodeId::ROOT).size, 32_100);
    assert_eq!(t.node(NodeId::ROOT).files, 3);
    assert_eq!(t.node(t.find(&["docs"]).unwrap()).size, 32_000);
    assert_eq!(out.progress.files, 3);
    assert_eq!(out.progress.dirs, 4, "root, docs, deep, empty");
    assert_eq!(out.progress.bytes, 32_100);
}

#[test]
fn hard_links_are_counted_once() {
    let fs = FakeFs::new(root())
        .hardlink("a/one.dll", 5000, 7)
        .hardlink("b/two.dll", 5000, 7)
        .hardlink("c/three.dll", 5000, 7)
        .file("c/other.dll", 10);
    let out = scan_blocking(fs, root(), opts(4));
    assert_eq!(out.tree.node(NodeId::ROOT).size, 5010);
    let dups = (0..out.tree.len() as u32)
        .map(NodeId::from_raw)
        .filter(|&n| out.tree.node(n).has(flags::HARDLINK_DUP))
        .count();
    assert_eq!(dups, 2);
}

#[test]
fn links_are_recorded_but_never_followed() {
    // The fake has no directory behind "jump"; listing it would produce a NotFound issue.
    let fs = FakeFs::new(root()).link_dir("jump").file("real/x", 5);
    let out = scan_blocking(fs, root(), opts(2));
    assert!(out.issues.is_empty(), "link was followed: {:?}", out.issues);
    let j = out.tree.find(&["jump"]).unwrap();
    assert!(out.tree.node(j).has(flags::LINK));
    assert_eq!(out.tree.node(j).size, 0);
}

#[test]
fn unreadable_folders_are_reported_and_siblings_still_scanned() {
    let fs = FakeFs::new(root())
        .file("ok/a", 10)
        .failing("secret", ListErrorKind::AccessDenied)
        .file("ok2/b", 20);
    let out = scan_blocking(fs, root(), opts(4));
    assert_eq!(out.status, ScanStatus::Completed);
    assert_eq!(out.tree.node(NodeId::ROOT).size, 30);
    assert_eq!(out.issues.len(), 1);
    assert_eq!(out.issues[0].error.kind, ListErrorKind::AccessDenied);
    assert_eq!(out.issues[0].path, root().join("secret"));
    let s = out.tree.find(&["secret"]).unwrap();
    assert!(out.tree.node(s).has(flags::UNREADABLE));
}

#[test]
fn partial_listings_keep_what_was_read() {
    let fs = FakeFs::new(root())
        .file("big/1", 1)
        .file("big/2", 2)
        .file("big/3", 4)
        .failing_after("big", 2, ListErrorKind::Other);
    let out = scan_blocking(fs, root(), opts(1));
    assert_eq!(out.tree.node(NodeId::ROOT).size, 3);
    assert_eq!(out.issues.len(), 1);
    assert!(out.issues[0].partial);
}

#[test]
fn drive_disappearing_ends_the_scan_but_keeps_the_partial_tree() {
    let fs = FakeFs::new(root())
        .file("first/a", 10)
        .failing("gone", ListErrorKind::VolumeGone);
    let out = scan_blocking(fs, root(), opts(1));
    assert_eq!(out.status, ScanStatus::VolumeLost);
    assert!(out.tree.len() > 1, "partial tree kept");
    assert!(
        out.issues
            .iter()
            .any(|i| i.error.kind == ListErrorKind::VolumeGone)
    );
}

#[test]
fn folders_swapped_mid_scan_are_not_followed() {
    let fs = FakeFs::new(root())
        .file("victim/inner/x", 99)
        .swapped("victim");
    let out = scan_blocking(fs, root(), opts(2));
    assert_eq!(out.tree.node(NodeId::ROOT).size, 0);
    assert_eq!(out.issues[0].error.kind, ListErrorKind::Changed);
    assert!(out.tree.find(&["victim", "inner"]).is_none());
}

#[test]
fn missing_root_is_reported() {
    let out = scan_blocking(FakeFs::new(root()), PathBuf::from("nope"), opts(2));
    assert!(matches!(out.status, ScanStatus::RootUnavailable(_)));
    assert_eq!(out.tree.len(), 1);
}

#[test]
fn cloud_placeholders_keep_their_flag_and_zero_local_allocation() {
    let fs = FakeFs::new(root()).file_with("OneDrive/movie.mp4", 4_000_000_000, 0, flags::CLOUD);
    let out = scan_blocking(fs, root(), opts(2));
    let m = out.tree.find(&["OneDrive", "movie.mp4"]).unwrap();
    assert!(out.tree.node(m).has(flags::CLOUD));
    assert_eq!(out.tree.node(NodeId::ROOT).size, 4_000_000_000);
    assert_eq!(out.tree.node(NodeId::ROOT).alloc, 0);
}

#[test]
fn cancellation_stops_promptly_and_keeps_partial_results() {
    let fs = wide_tree("R", 2000, 3).slow(Duration::from_millis(2));
    let handle = start(fs, root(), opts(2));
    std::thread::sleep(Duration::from_millis(60));
    handle.cancel();
    let out = handle.wait();
    assert_eq!(out.status, ScanStatus::Cancelled);
    assert!(out.progress.dirs < 2100, "scan should have stopped early");
    assert!(out.progress.elapsed < Duration::from_secs(5));
}

#[test]
fn partial_tree_is_visible_while_scanning() {
    let fs = wide_tree("R", 400, 2).slow(Duration::from_millis(1));
    let handle = start(fs, root(), opts(2));
    std::thread::sleep(Duration::from_millis(40));
    let (len, size) = handle.with_tree(|t| (t.len(), t.node(NodeId::ROOT).size));
    assert!(len > 1);
    let out = handle.wait();
    assert!(out.tree.node(NodeId::ROOT).size >= size);
}

#[test]
fn results_do_not_depend_on_thread_count() {
    let a = scan_blocking(wide_tree("R", 300, 10), root(), opts(1));
    let b = scan_blocking(wide_tree("R", 300, 10), root(), opts(8));
    let ra = a.tree.node(NodeId::ROOT);
    let rb = b.tree.node(NodeId::ROOT);
    assert_eq!((ra.size, ra.alloc, ra.files), (rb.size, rb.alloc, rb.files));
    assert_eq!(a.tree.len(), b.tree.len());
}

#[test]
fn half_a_million_files_fit_the_memory_budget() {
    let out = scan_blocking(wide_tree("R", 5_000, 100), root(), ScanOptions::default());
    assert_eq!(out.status, ScanStatus::Completed);
    assert_eq!(out.progress.files, 500_000);
    let per_node = out.tree.memory_bytes() / out.tree.len();
    assert!(per_node <= 100, "{per_node} bytes per node");
}

#[test]
fn progress_matches_the_final_tree() {
    let out = scan_blocking(wide_tree("R", 50, 7), root(), opts(4));
    let r = out.tree.node(NodeId::ROOT);
    assert_eq!(out.progress.files, u64::from(r.files));
    assert_eq!(out.progress.bytes, r.size);
    assert_eq!(out.progress.alloc, r.alloc);
}
