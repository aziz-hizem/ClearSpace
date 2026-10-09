# Spike: making "Move to Recycle Bin" fail instead of silently deleting permanently

**Status:** results pending (see "Results" below)  ·  **Milestone:** (a)  ·  **Code:**
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

_Pending the CI run._
