# ClearSpace (working name)

A Windows 10/11 disk-space analyzer with a Cleanup Assistant that explains, in plain language,
what is safe to remove — and **never deletes anything without your review and confirmation**.

> Status: early development (milestone a — scanner). Not ready for end users.

## Repository layout

| Path | What it is |
|---|---|
| `crates/model` | Memory-efficient file tree (≈60 bytes per entry), aggregation, CSV export |
| `crates/scanner` | Parallel, cancellable, read-only directory scanner (works without admin) |
| `crates/scan-cli` | `cs-scan`, a developer CLI for trying the scanner |
| `crates/cleaner` | The only component allowed to delete (currently just the Recycle Bin spike) |
| `docs/spikes/` | Technical investigations and their results |
| `CLAUDE.md` | Safety principles and rules for every contributor (human or AI) |

## Building

Requires Rust 1.97+ (`rustup` stable). On Windows, the MSVC build tools.

```powershell
cargo test --workspace
cargo run --release -p clearspace-scan-cli -- C: --top 20
cargo run --release -p clearspace-scan-cli -- "$env:USERPROFILE" --csv profile.csv
```

`cs-scan` only reads. It never changes anything except the CSV file you ask for, and it
refuses to overwrite an existing file.

## Supported platforms

Windows 10 22H2 and Windows 11 24H2 or later, x64. ARM64 builds are compiled in CI but are
experimental and untested.
