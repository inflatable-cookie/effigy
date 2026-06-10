# Incremental Index Short Path

Date: 2026-05-18  
Roadmap: [`g07.014`](../roadmaps/g07/014-incremental-indexing-and-cache-reuse.md)  
Batch card: [`932`](../roadmaps/g07/batch-cards/932-implement-incremental-index-short-path.md)  
Strict lane: [`086`](../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md)

## What Changed

- stopped `run_index()` from clearing and rebuilding the entire graph on every
  run
- added per-file graph deletion helpers so changed and deleted files can be
  replaced surgically
- reused existing graph records when path, language, content hash, and
  extractor version are still compatible
- preserved no-op correctness for mtime-only rewrites by updating file scan
  state without rerunning extractors
- stopped rebuilding the FTS search index on true no-op runs
- added regression coverage for:
  - mtime-only unchanged-content reindex
  - deleted-file graph cleanup on reindex

## Measured Delta

### Full Effigy Repo No-Op Reindex

- `g07.012` baseline: `148.39s`
- after `932`: `26.84s`

That is an `81.9%` reduction in wall-clock no-op index cost.

### Changed-Slice Fixture Reindex

- representative two-file fixture with one changed Rust file: `0.13s`

## Validation

- `cargo test -p effigy-codegraph`
- `cargo test graph -- --nocapture`
- `cargo fmt --all -- --check`

## Remaining Limits

- the repo still scans the file tree every run
- `status`, `search`, and `context` still over-materialize data on the query
  side
- the seven failed full-repo fixture paths remain for `934`

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`, `CONTRACT`
- moved: full rebuild on every `graph index` -> file-level incremental reuse
  with a real no-op fast path
- remains open: `933`, `934`, `935`
