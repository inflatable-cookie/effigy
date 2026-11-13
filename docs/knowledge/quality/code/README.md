# Code-module quality tables at `a9b3213f9`

The frozen, calibrated product-code ratings: 707 modules in `C1.csv`–`C5.csv`
(C1 containers, C2 gateway/host/managed, C3 kernel/custody, C4 CLI/config/engine/UI,
C5 remaining crates and root), rated at commit `a9b3213f9`. Method, calibration
and distribution are in `SUMMARY.md`; the rating rubric is the code-rating charter
beside the frozen source.

## Columns

| Column | Meaning |
|---|---|
| `id` | Stable module id: the rated Rust path (`gateway::atomic_write`) or repo-relative file/group path (`crates/effigy-tui/src/multiprocess/render`). Unique across all five tables. |
| `files` | Exact file set covered at `a9b3213f9`, `;`-separated in sorted order: one file, or a directory module's files. Test-only files (`tests/` trees, `tests.rs`, `test_support`) are excluded unless the rater explicitly kept them; three entries overlap a nested row's files (noted in `SUMMARY.md`). |
| `loc` | Rated lines of code, copied verbatim from the frozen table. |
| `given` / `corrected` | Score as rated, and the calibration-corrected score. The re-rate works from `corrected`; never edit these. |
| `source` | `blind` (independent re-score) or `offset` (domain/large-file correction). |
| `action` / `reason` | Rated disposition and its evidence, copied verbatim. |
| `body_sha` | sha256 over the concatenated raw bytes of `files` at `a9b3213f9`, in sorted path order (no separator). For single-file modules this is the file's sha256. |

## Drift

`effigy qa:quality:code [-- --summary]` (`scripts/quality-table.py --code`)
recomputes each row's body over the working tree and reports rows whose file
set changed (body differs), rows with missing files, and product modules with
no row, plus `split: re-rate` groups where new files sit under a gone row's
path. Report only; it never gates QA. Scores stay frozen: the re-rate is a
separate step that writes new tables, not edits to these.
