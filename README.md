# ClearSpace (working name)

A Windows 10/11 disk-space analyzer with a Cleanup Assistant that explains, in plain language,
what is safe to remove — and **never deletes anything without your review and confirmation**.

> Status: early development (milestone b: app shell + treemap). Not ready for end users.
> The name is a working codename; product identity lives in `product.json`.

## Repository layout

| Path | What it is |
|---|---|
| `crates/model` | Memory-efficient file tree (≈60 bytes per entry), aggregation, CSV export |
| `crates/scanner` | Parallel, cancellable, read-only directory scanner (works without admin) |
| `crates/scan-cli` | `cs-scan`, a developer CLI for trying the scanner |
| `crates/treemap` | Squarified treemap layout, computed next to the data and sent to the UI as a compact binary buffer |
| `crates/service` | Scan sessions and the read-only queries the UI makes (no UI-framework dependency, fully tested) |
| `crates/app` | The Tauri v2 desktop app: a thin IPC layer over `service` |
| `ui/` | React + TypeScript + Tailwind UI (Vite). Runs in the app, or in a browser with clearly labelled sample data |
| `product.json` | Display name, bundle ID, publisher, installer name — the single place to rename the product |
| `crates/cleaner` | The only component allowed to delete (currently just the Recycle Bin spike) |
| `docs/spikes/` | Technical investigations and their results |
| `CLAUDE.md` | Safety principles and rules for every contributor (human or AI) |

## Building

Requires Rust 1.97+ (`rustup` stable), Node.js 24 LTS, and on Windows the MSVC build tools and
WebView2 (built into Windows 11; usually present on Windows 10).

```powershell
npm ci
npm run tauri dev                    # run the desktop app with hot reload
npm run tauri -- build --bundles nsis  # build ClearSpace.exe + an (unsigned) installer
npm test; npm run lint; npm run typecheck
cargo test --workspace
cargo run --release -p clearspace-scan-cli -- C: --top 20
```

UI-only development without the Rust side: `npm run dev` and open http://localhost:1420. The
browser preview uses sample data and says so in a banner.

### Renaming the product

Edit `product.json`, then `npm run product:sync` (also run automatically by `npm run tauri`).
CI fails if any derived file is out of sync.

`cs-scan` only reads. It never changes anything except the CSV file you ask for, and it
refuses to overwrite an existing file.

## Supported platforms

Windows 10 22H2 and Windows 11 24H2 or later, x64. ARM64 builds are compiled in CI but are
experimental and untested.
