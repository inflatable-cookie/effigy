# Tighten Stale And Status Scan Cost

Date: 2026-05-18  
Roadmap: [`g07.019`](../roadmaps/g07/019-safe-scan-metadata-reuse.md)  
Batch card: [`953`](../roadmaps/g07/batch-cards/953-tighten-stale-and-status-scan-cost.md)  
Strict lane: [`087`](../specs/087-graph-scan-cost-reduction-strict-lane.md)

## What Changed

- collapsed `graph status` path classification onto one repo scan snapshot
- replaced separate `new`, `changed`, `deleted`, and `stale` repo walks with a
  single `ScanDelta` pass
- kept the public `graph status --json` payload unchanged

## Measured Delta

Clean serial timings on the current worktree:

- `graph status --json`
  - before `953`: `0.48s`
  - after `953`: `0.21s` to `0.24s`
  - representative improvement: about `56%`

For reference:

- `graph index --json` after `952`: `0.25s`

## Interpretation

- the duplicated status scans were worth removing
- the remaining graph scan floor is now small enough that another scan-specific
  execution card would be chasing marginal wins

## Validation

- `cargo test -p effigy-codegraph`
- `cargo test graph -- --nocapture`
- `cargo build --bin effigy`
- repeated clean `./target/debug/effigy graph status --json`
- clean `./target/debug/effigy graph index --json`
- `git diff --check`

## Remaining Limits

- no-op graph commands still pay for at least one full repo walk plus metadata
  collection
- further savings are likely to be small without riskier cache or watcher
  designs

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- moved: `graph status` four-scan path -> one-scan path with a stable
  sub-quarter-second runtime
- remains open: `954`
