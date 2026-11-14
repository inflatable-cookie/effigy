# Code-module quality tables

The product-code ratings: 815 modules in `C1.csv`–`C5.csv` (C1 containers 200,
C2 gateway/host/managed 73, C3 kernel/custody 110, C4 CLI/config/engine/UI 189,
C5 remaining crates and root 243). They were rated at commit `a9b3213f9`,
re-rated at `e458614d8` after round 5's splits and re-rated again at current main
after round 6 and floor 7. Method, calibration, distribution, and the round-5
and floor-7 re-rates are in `SUMMARY.md`; the rating rubric is the code-rating
charter beside the frozen source.

## Columns

| Column | Meaning |
|---|---|
| `id` | Stable module id: the rated Rust path (`gateway::atomic_write`) or repo-relative file/group path (`crates/effigy-tui/src/multiprocess/render`). Unique across all five tables. |
| `files` | Exact file set the row covers, `;`-separated in sorted order: one file, or a directory module's files. Test-only files (`tests/` trees, `tests.rs`, `test_support`) are excluded unless the rater explicitly kept them; three entries overlap a nested row's files (noted in `SUMMARY.md`). |
| `loc` | Rated lines of code. Rows re-rated after round 5 carry their current line counts (a group row counts its files minus the files a nested row owns); rows unchanged since `a9b3213f9` keep the frozen counts verbatim. The floor-7 re-rate recomputes `loc` only for the rows it re-rated, so an untouched row keeps the count from its own rating pass. |
| `given` / `corrected` | Score as rated, and the calibration-corrected score. The re-rate works from `corrected`; never edit these. |
| `source` | Which calibration produced `corrected`: `blind` (independent re-score at `a9b3213f9`), `offset` (domain/large-file correction at `a9b3213f9`), `blind-r5` (independent blind re-score of a row re-rated after round 5), `offset-r5` (uniform −1 on a row re-rated after round 5) or `offset-f7` (uniform −1 on a row re-rated in the floor-7 re-rate). The 555 rows frozen at `a9b3213f9` keep `blind`/`offset`; the 244 round-5 rows carry the `-r5` pair. |
| `action` / `reason` | Rated disposition and its evidence, with `file:line` for the mechanism the row names. |
| `body_sha` | sha256 over the concatenated raw bytes of `files` in sorted path order (no separator). For single-file modules this is the file's sha256. |

## Drift

`effigy qa:quality:code [-- --summary]` (`scripts/quality-table.py --code`)
recomputes each row's body over the working tree and reports rows whose file
set changed (body differs), rows with missing files, and product modules with
no row, plus `split: re-rate` groups where new files sit under a gone row's
path. Report only; it never gates QA. A re-rate is a separate step that writes
new tables, not edits to these.

The floor-7 re-rate left the tables at 0 drift:

```text
code quality: 815 rows, 0 new, 0 changed, 0 gone, 0.0% stale since e458614d8
```
