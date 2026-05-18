# Reduce Repeated Scan Work

Date: 2026-05-18  
Roadmap: [`g07.019`](../roadmaps/g07/019-safe-scan-metadata-reuse.md)  
Batch card: [`952`](../roadmaps/g07/batch-cards/952-reduce-repeated-scan-work.md)  
Strict lane: [`087`](../specs/087-graph-scan-cost-reduction-strict-lane.md)

## What Changed

- stopped `graph index` from rescanning the repo to compute stale paths after
  it already had the current scan entries in hand
- extracted stale-path calculation so it can operate on an existing
  `ScanEntry` slice instead of forcing a second walk
- kept `graph status` unchanged for now so `953` still has a bounded target

## Measured Delta

Clean no-op timings on the current worktree:

- `graph index --json`
  - before `952`: `0.39s`
  - after `952`: `0.32s`
  - improvement: `17.9%`

For comparison, the direct walk baseline from `951` remained roughly
`40–50ms` per pass, which matches the size of this win.

## Interpretation

- the duplicated no-op walk inside `graph index` was worth removing
- the win is real but modest, which confirms the `951` conclusion: remaining
  scan work is now polish, not a major performance defect

## Validation

- `cargo test -p effigy-codegraph`
- `cargo test graph -- --nocapture`
- `cargo build --bin effigy`
- clean `./target/debug/effigy graph index --json`
- `git diff --check`

## Remaining Limits

- `graph status` still performs multiple repo walks and remains the next
  obvious scan-path cleanup
- no-op index still pays for one full repo scan plus metadata collection

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- moved: duplicated no-op `graph index` walk -> single-scan stale calculation
  with a measurable timing drop
- remains open: `953`, `954`
