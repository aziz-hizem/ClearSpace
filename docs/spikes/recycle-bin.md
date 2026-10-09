# Spike: making "Move to Recycle Bin" fail instead of silently deleting permanently

**Status:** complete — awaiting owner sign-off  ·  **Milestone:** (a)  ·  **Code:**
`crates/cleaner/examples/recycle_spike.rs`  ·  **Runs in:** GitHub Actions `windows-latest`
and, for the non-registry scenarios, on any Windows PC.

## Why this matters

ClearSpace promises that the default removal method is the Recycle Bin, so mistakes can be
undone. Windows does **not** always honour a "recycle" request. It deletes permanently,
sometimes with only a dialog and sometimes silently, when:

* the Recycle Bin is turned off for that drive ("Don't move files to the Recycle Bin"),
* the item is larger than the drive's Recycle Bin quota,
* the item is on a network share or on media without a Recycle Bin,
* the path is something the shell can't recycle (to be measured: long paths).

If any of these happen silently, our core safety promise breaks. The spike measures how each
candidate API behaves in each case.

## What Microsoft documents

Docs were read through the MicrosoftDocs GitHub mirror of the public pages:

* `FOF_NOCONFIRMATION` means *"Respond with Yes to All for any dialog box that is displayed."*
  That includes the "This file is too big to recycle. Delete it permanently?" prompt.
* `FOF_WANTNUKEWARNING` means *"Send a warning if a file or folder is being destroyed during a
  delete operation rather than recycled. This flag partially overrides FOF_NOCONFIRMATION."*
  The word "partially" isn't explained further.
* `FOFX_RECYCLEONDELETE` (Windows 8+) means *"When a file is deleted, send it to the Recycle
  Bin rather than permanently deleting it."*
* `IFileOperationProgressSink::PreDeleteItem` receives `TRANSFER_SOURCE_FLAGS`, which
  include `TSF_DELETE_RECYCLE_IF_POSSIBLE`. Returning an error *"cancels the delete operation
  and all subsequent operations pending from the call."*
* `IFileOperationProgressSink::PostDeleteItem` receives `psiNewlyCreated`, which is *"the
  deleted item, now in the Recycle Bin. If the item was fully deleted, this value is NULL."*

Sources:
<https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-setoperationflags>,
<https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperationprogresssink-predeleteitem>,
<https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperationprogresssink-postdeleteitem>
(read 2026-10-09).

## Candidates

| | Approach | Flags |
|---|---|---|
| **A** | `trash` crate 5.2.9, `trash::delete` | `FOF_NO_UI \| FOF_ALLOWUNDO \| FOF_WANTNUKEWARNING` (from its source) |
| **B** | `IFileOperation` called directly, plus our progress sink | `FOF_SILENT \| FOF_NOCONFIRMATION \| FOF_NOERRORUI \| FOF_NOCONFIRMMKDIR \| FOF_ALLOWUNDO \| FOFX_RECYCLEONDELETE`. The sink returns `E_ABORT` from `PreDeleteItem` whenever `TSF_DELETE_RECYCLE_IF_POSSIBLE` is missing |

Each scenario runs in a child process with a 60-second timeout, so a modal dialog shows up as
"HUNG" instead of blocking the run.

## Scenarios

Small file, nested folder, locked file (open with no sharing), read-only file, path over 260
characters, Unicode name. CI only: Recycle Bin disabled for the volume (`NukeOnDelete=1`),
quota exceeded (`MaxCapacity=1` MB with an 8 MB file), and a file addressed through
`\\localhost\C$`. Registry values are per-user, per-volume under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket\Volume\{GUID}` and are
restored afterwards. A performance run compares batch and one-item-per-operation calls on
300 files.

For every run we record whether the item was kept, recycled (gone, and listed in the bin
under its original path) or **permanently deleted** (gone and not in the bin). We also record
whether restoring it from the bin works, which is the basis for Undo.

## Results

Windows 11 24H2 (build 26100.33438), GitHub Actions `windows-latest`, CI run
[#5](https://github.com/aziz-hizem/ClearSpace/actions/runs/37994552080). All files were
sandbox files the spike created itself.

| Scenario | A: `trash` crate | B: IFileOperation + veto sink | B2: B + `FOF_WANTNUKEWARNING` |
|---|---|---|---|
| Small file / read-only / Unicode name / path > 260 chars | recycled | recycled | recycled |
| Folder with nested files | recycled | recycled | recycled |
| File locked by another handle | kept, generic error | kept, `0x80270027` (in use) | kept, `0x80270027` |
| **Recycle Bin turned off** | ❌ **silently deleted permanently** | ✅ kept (veto: flags `0x202` lack the recycle bit) | ✅ kept |
| **Network path** (`\\localhost\C$`) | ❌ **silently deleted permanently** | ✅ kept (veto) | ✅ kept |
| **Single file larger than the bin quota** | ⚠️ modal dialog (hung 60 s) | ❌ **silently deleted permanently**: flags still said "recycle" (`0x282`); the shell later reported no bin item | ⚠️ modal dialog |
| Folder larger than the quota (each file small) | ⚠️ modal dialog | ✅ kept (Windows fell back to per-item delete with flags `0x2`, and we vetoed) | ⚠️ modal dialog |
| Bin nearly full (9 of 10 MB used, then a 5 MB file) | recycled; older items kept 3/3 | same | same |
| Undo: restore a recycled **folder** | ✅ | ✅ | ✅ |
| Undo: restore a recycled `.txt` file (via `trash` restore) | ❌ reports success, but nothing is found at the original path afterwards | ❌ same (restore goes through the crate) | ❌ same |
| Undo: restore a file whose path is > 260 chars | ❌ `E_INVALIDARG` | ❌ same | ❌ same |

Performance (300 small files): `trash::delete_all` 1.1–1.7 s, one IFileOperation batch
1.0–1.4 s, one operation per file 3.1–4.3 s.

### Side findings

* **The `trash` crate's bin listing drops file extensions.** It reports Explorer's *display*
  name, which hides extensions of known file types by default (`small.txt` is listed as
  `small`). That is why its restore can't find files under their real name. Our Undo must not
  depend on display names.
* **8.3 short paths.** On the CI runner `%TEMP%` is `C:\Users\RUNNER~1\…`, while the Recycle Bin
  records `C:\Users\runneradmin\…`. The first spike run compared paths textually and wrongly
  reported every recycled file as permanently deleted. Path canonicalisation (long names, case,
  `\\?\` prefixes) is therefore a hard requirement for `crates/safety`, both for the guard and
  for matching history and Undo records.
* `PreDeleteItem` returning `E_ABORT` cancels the *whole remaining batch* (as documented), so
  one vetoed item stops everything queued after it.

## Decision (proposed; needs your OK before the cleaner is built)

1. **Don't use the `trash` crate** for deleting or restoring. It silently deletes permanently in
   two common situations and its restore is unreliable. It stays only as a dev-dependency of
   this spike.
2. **Use IFileOperation directly with the veto sink (approach B), without
   `FOF_WANTNUKEWARNING`.** That flag only produces a modal Windows dialog, which we can't
   control from our UI.
3. **Close the remaining hole (one file bigger than the bin quota) in three layers:**
   * **Before confirmation (plan build):** for every item, check the drive type (local fixed
     volume), `NukeOnDelete`, and item size against the volume's bin capacity (`MaxCapacity`,
     with a safety margin: anything ≥ 90 % of capacity counts as "too big"). Items that fail
     are shown on the Review screen as **"Can't go to the Recycle Bin"**, excluded by default,
     and only deletable through the explicit permanent-delete opt-in with its extra
     confirmation.
   * **At the last moment (`PreDeleteItem`):** re-read the bin settings and compare against the
     planned size of *that* item. Veto if it no longer fits. This shrinks the window between
     check and delete to milliseconds.
   * **After (`PostDeleteItem`):** if an item meant for the bin comes back with no bin item, the
     Result screen says plainly that Windows deleted it permanently. This is a last line of
     honesty, not a guard; layers 1–2 should make it unreachable.
4. **Batching:** delete in batches. When the sink vetoes, record that item as "not recycled
   (reason)" and resume with the rest in a new operation.
5. **Undo:** record the exact bin item path that `PostDeleteItem` hands back
   (`C:\$Recycle.Bin\<SID>\$R…`) together with the original full path. Restore through the
   shell using that exact item, never by matching display names. Long-path restore must be
   proven in milestone (e) before Undo ships; if it can't be made reliable, those items are
   labelled "Undo not available" *before* confirmation.
6. **Locked files:** `0x80270027` and similar map to "Skipped: in use by another app".

### Open items for milestone (e)

* Confirm how Windows sets a volume's default bin capacity when `MaxCapacity` is absent (the
  runner had 9,699 MB on a ~150 GB disk), and find a citable source. If none is found, treat
  "no value" as "unknown" and fall back to a conservative cap.
* Prove a restore path for files with known extensions and for paths longer than 260
  characters.
* Removable drives (USB sticks) and ReFS / Dev Drive volumes.
