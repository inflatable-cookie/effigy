# File Walk And Scan Cost Baseline

Date: 2026-05-18  
Roadmap: [`g07.018`](../roadmaps/g07/018-file-walk-and-scan-metadata-baseline.md)  
Batch card: [`951`](../roadmaps/g07/batch-cards/951-baseline-file-walk-and-scan-cost.md)  
Strict lane: [`087`](../specs/087-graph-scan-cost-reduction-strict-lane.md)

## What Changed

- measured the graph scan floor directly instead of inferring it from full
  command timings
- classified the current duplicated repo walks in `graph index` and
  `graph status`
- compared the raw walk/metadata cost to the current no-op command surface

## Measurements

Ad hoc walk benchmark against the Effigy repo root:

- walk only, count file entries: `50ms`
  - `4090` files seen by the ignore walker before graph filtering
- walk + graph path/language filtering: `43ms`
  - `3220` candidate graph files
- walk + filtering + metadata collection: `50ms`
  - `3220` candidate graph files

Current clean no-op command timings on `94ccf42e`:

- `graph index --json`: `0.39s`
- `graph status --json`: `0.49s`

## Structural Findings

- `graph index` still walks the repo twice in the no-op case:
  1. `scan_repo_files()` in `run_index()`
  2. `scan_repo_files()` again inside `stale_paths_for_repo()`
- `graph status` still walks the repo four times:
  1. `stale_paths_for_repo()`
  2. `current_new_paths()`
  3. `current_changed_paths()`
  4. `current_deleted_paths()`

## Interpretation

- raw file walking is no longer the dominant graph cost
- duplicated scan passes are real, but each pass is now on the order of
  `40–50ms`, not seconds
- even a perfect removal of repeated scan passes is now a small win, probably
  measured in low hundreds of milliseconds rather than order-of-magnitude
  improvements

## Decision Posture

This means the original motivation for a dedicated scan-cost lane weakened
substantially after the latest graph hardening landed.

There is still room to:

- collapse duplicate repo walks inside `graph index`
- collapse duplicate repo walks inside `graph status`

But this is now a polish tradeoff, not an urgent performance problem.

## Validation

- ad hoc `ignore::WalkBuilder` benchmark against the repo root
- `./target/debug/effigy graph status --json`
- `./target/debug/effigy graph index --json`
- static inspection of `crates/effigy-codegraph/src/index.rs`

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- moved: inferred scan-cost suspicion -> direct proof that repo walking is
  duplicated but now relatively cheap
- remains open: `952`, `953`, `954` only if sub-`200ms` graph polish is worth
  another code batch
