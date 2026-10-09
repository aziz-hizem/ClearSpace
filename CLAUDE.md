# ClearSpace — instructions for every session

ClearSpace is a Windows 10/11 disk analyzer with a Cleanup Assistant that **explains and
recommends**; it never cleans on its own. "ClearSpace" is a working codename (a Windows
cleanup app named "ClearSpace Disk" already exists); keep the product name in one place so a
rename stays cheap.

## Safety principles (override everything else)

1. **Nothing is deleted without explicit user review and confirmation.** No auto-clean,
   background cleaning or one-click clean that skips review. The app only recommends.
2. **Deletion code lives only in `crates/cleaner`.** It accepts only a `DeletionPlan` created
   exclusively by the user's final confirmation in the UI. No other code may delete, move or
   modify user files. Enforced by `clippy.toml` (`disallowed_methods`, denied workspace-wide;
   only the cleaner crate opts out) and by `crates/cleaner/tests/deletion_audit.rs`, which
   scans the source tree for Win32/COM deletion APIs.
3. **Recycle Bin by default.** Permanent deletion is an explicit per-run opt-in with an extra
   confirmation. Where recycling is impossible (bin disabled, item over quota, removable or
   network drive), say so clearly *before* the user confirms — never silently delete
   permanently. See `docs/spikes/recycle-bin.md`: never use the `trash` crate (it silently
   deletes permanently when the bin is off or the path is on a network share, and its
   restore is unreliable). Recycle via IFileOperation + the veto progress sink, plus a quota
   pre-flight and a last-moment size check.
4. **Unknown means untouched.** Only items matched by a curated rule can be recommended.
   Unmatched items are "Unknown, not recommended for deletion" and can only be inspected.
   The user's own documents/photos/videos are never labelled safe; they appear only in
   user-judged views (Large files, Duplicates) marked "Your files, your call".
5. **Never directly delete system-managed data** (WinSxS, Windows\Installer, System Volume
   Information, hiberfil.sys, pagefile.sys, Windows.old internals, …). Explain, and offer to
   launch Windows' own tool (Storage Sense, Disk Cleanup, DISM) after confirmation.
   Admin-only items (Windows Update cache, Windows.old, system temp) are `windows-tool`
   items: the unelevated cleaner never touches them.
6. **Protected-path guard** (`crates/safety`): hard-coded denylist checked at plan-build
   time AND immediately before each deletion. Canonicalise, resolve symlinks/junctions,
   never follow reparse points out of scope, reject traversal, verify the path is still in
   the rule's allowed scope.
7. **Cloud placeholders** (OneDrive etc.): never recommend deleting in cloud-synced folders
   without a loud warning that it deletes from the cloud. Never hydrate placeholders while
   scanning (no reads of file contents; duplicates skip placeholders).
8. **Skip locked / in-use files** and report them afterwards; never force.
9. **Offline and private.** No telemetry, no network calls. (v1 has no update check. A
   signed, opt-in rule update may be added later; the rule loader is designed for it.)

## Process model

- `ClearSpace.exe` runs **unelevated** and contains the UI, rules and the cleaner.
- `clearspace-scan-helper.exe` (milestone c) is the only elevated process: read-only MFT
  scanning, results over a user-ACL'd named pipe. It must never depend on `clearspace-cleaner`.

## Development rules

- **Never run the cleaner (or the Recycle Bin spike) against real user data.** Only fixtures
  and sandbox directories the code itself created. The spike's registry-changing scenarios
  run only with `CLEARSPACE_SPIKE_CI=1` (GitHub Actions) and restore the registry afterwards.
- Check current official docs / latest stable versions before adding a dependency or API.
  `learn.microsoft.com` is unreachable from the dev container; read Microsoft docs through
  `https://raw.githubusercontent.com/MicrosoftDocs/sdk-api/docs/...` or `MicrosoftDocs/win32`.
- Every cleanup rule must cite a source URL and `verified_on` date. If a path cannot be
  confirmed safe, leave it out or make it advise-only. Never guess.
- Don't copy code, assets or branding from WizTree or any other product.
- Compare paths only after canonicalising them (long names, not 8.3 `RUNNER~1`; case;
  `\\?\` prefix). The first Recycle Bin spike run was fooled by exactly this.
- Code must be testable off-Windows where possible: platform-independent logic sits behind
  traits (`scanner::FsSource`); Windows code is `#[cfg(windows)]` and exercised in CI on
  `windows-latest`. Locally run:
  `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  and `cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings`.
- Test fixtures that need link creation must `#[allow(clippy::disallowed_methods)]` locally,
  inside test code only, with a comment.
- Targets: Windows 10 22H2 and Windows 11 24H2+, x64. ARM64 is compile-only (experimental).
- Commit per milestone step on the designated branch; stop after each milestone with a
  precise Windows test checklist for the owner.

## Layout

- `crates/model` — arena file tree (48-byte nodes), aggregation, queries, CSV export.
- `crates/scanner` — parallel directory scan behind `FsSource` (Windows / portable / fake).
- `crates/scan-cli` — `cs-scan` developer CLI for exercising the scanner.
- `crates/cleaner` — the only deleter (currently: Recycle Bin spike in `examples/`).
- Planned: `mft`, `rules`, `safety`, `app` (Tauri), `scan-helper`, `ui/` (React).
