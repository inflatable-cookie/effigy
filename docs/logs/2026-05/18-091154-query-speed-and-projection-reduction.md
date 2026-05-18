# Query Speed And Projection Reduction

Date: 2026-05-18  
Roadmap: [`g07.015`](../roadmaps/g07/015-query-speed-and-projection-reduction.md)  
Batch card: [`933`](../roadmaps/g07/batch-cards/933-reduce-query-latency-and-context-projection-cost.md)  
Strict lane: [`086`](../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md)

## What Changed

- removed avoidable whole-record scans for `find_file_by_id` and
  `find_symbol_by_id` by switching them to direct SQL lookups
- made stale detection short-circuit on unchanged metadata before hashing file
  content
- made `current_changed_paths()` metadata-driven instead of content-hash driven
  because the contract only needs to report changed paths, not prove semantic
  drift

## Measured Delta

Compared with the locked `g07.012` baseline:

- `graph status --json`
  - baseline: `2.34s`
  - after `933`: `1.72s`
  - improvement: `26.5%`
- `graph search release --limit 5 --json`
  - baseline: `4.43s`
  - after `933`: `3.96s`
  - improvement: `10.6%`
- `graph context "trace release orchestrator" --language rust --max-files 6 --max-bytes 2048 --json`
  - baseline: `2.73s`
  - after `933`: `2.35s`
  - improvement: `13.9%`

## Validation

- `cargo test -p effigy-codegraph`
- `cargo test graph -- --nocapture`
- `cargo fmt --all -- --check`

## Remaining Limits

- `context` still materializes full file/symbol/edge collections before ranking
- `search` still depends on thin follow-up record reads per match
- the seven failed full-repo fixture paths remain for `934`

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- moved: expensive query-time hashing and record lookups -> thinner status and
  search paths with measurable latency reduction
- remains open: `934`, `935`
