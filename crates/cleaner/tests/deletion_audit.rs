//! Safety principle 2, enforced as a test: deletion code lives only in `crates/cleaner`.
//!
//! Clippy's `disallowed_methods` (see `clippy.toml`) catches the std/trash APIs, but not raw
//! Win32/COM calls, FFI declarations or `NtSetInformationFile` tricks. This test scans every
//! Rust source outside the cleaner crate for those tokens, and checks that only approved
//! crates depend on the cleaner (or on the `trash` crate).

use std::fs;
use std::path::{Path, PathBuf};

/// Tokens that mean "this code can delete, move or modify a file". Matched on code with
/// `//` comments stripped.
const FORBIDDEN: &[&str] = &[
    "remove_file",
    "remove_dir",
    "fs::rename",
    "set_permissions",
    "DeleteFileW",
    "DeleteFileA",
    "DeleteFile2",
    "RemoveDirectoryW",
    "RemoveDirectoryA",
    "MoveFileW",
    "MoveFileExW",
    "MoveFileA",
    "MoveFileExA",
    "MoveFileWithProgress",
    "ReplaceFileW",
    "SHFileOperation",
    "IFileOperation",
    "FileDispositionInfo",
    "FILE_DISPOSITION_INFO",
    "FILE_FLAG_DELETE_ON_CLOSE",
    "FileRenameInfo",
    "NtSetInformationFile",
    "SetFileAttributes",
    "SetFileInformationByHandle",
    "trash::",
];

/// Crates allowed to depend on the cleaner: only the app, which builds a `DeletionPlan` from
/// the user's confirmation. The elevated scan helper must never be on this list.
const MAY_DEPEND_ON_CLEANER: &[&str] = &["crates/app"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name == "target" || name == "node_modules" || name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            rust_files(&p, out);
        } else if name.ends_with(".rs") {
            out.push(p);
        }
    }
}

/// Returns `(line number, token)` for each forbidden token in `source`.
fn violations(source: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        for tok in FORBIDDEN {
            if code.contains(tok) {
                found.push((i + 1, *tok));
            }
        }
    }
    found
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

#[test]
fn no_deletion_code_outside_the_cleaner() {
    let root = workspace_root();
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    assert!(files.len() > 5, "audit found too few files; is the root right? {root:?}");
    let mut report = Vec::new();
    for f in &files {
        let r = rel(&root, f);
        if r.starts_with("crates/cleaner/") {
            continue;
        }
        let src = fs::read_to_string(f).unwrap();
        for (line, tok) in violations(&src) {
            report.push(format!("{r}:{line}: `{tok}`"));
        }
    }
    assert!(
        report.is_empty(),
        "Deletion/modification APIs found outside crates/cleaner (SAFETY.md principle 2):\n{}",
        report.join("\n")
    );
}

#[test]
fn only_approved_crates_depend_on_the_cleaner_or_trash() {
    let root = workspace_root();
    let mut report = Vec::new();
    for dir in fs::read_dir(root.join("crates")).unwrap().flatten() {
        let manifest = dir.path().join("Cargo.toml");
        let Ok(text) = fs::read_to_string(&manifest) else { continue };
        let r = rel(&root, &dir.path());
        if r == "crates/cleaner" {
            continue;
        }
        let deps_cleaner = text.lines().any(|l| l.trim_start().starts_with("clearspace-cleaner"));
        let deps_trash = text.lines().any(|l| l.trim_start().starts_with("trash"));
        if deps_trash {
            report.push(format!("{r} depends on `trash`"));
        }
        if deps_cleaner && !MAY_DEPEND_ON_CLEANER.contains(&r.as_str()) {
            report.push(format!("{r} depends on clearspace-cleaner"));
        }
    }
    assert!(report.is_empty(), "{}", report.join("\n"));
}

#[test]
fn the_audit_actually_detects_things() {
    assert_eq!(violations("std::fs::remove_file(p);"), vec![(1, "remove_file")]);
    assert_eq!(violations("let x = 1; // remove_file in a comment is fine"), vec![]);
    assert!(!violations("unsafe { DeleteFileW(w) }").is_empty());
    assert!(!violations("op.DeleteItem(&i, None)?; let o: IFileOperation = x;").is_empty());
}
