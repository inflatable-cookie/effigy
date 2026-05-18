# g07.017 - Graph Scan Cost Reduction Suite

Status: Complete
Depends on: `g07.001` through `g07.016`

## Goal

Reduce the remaining no-op `graph index` cost that now appears to be dominated
by file walking and scan metadata work rather than extractor execution.

## Why This Exists

`g07.013` made no-op indexing cheap enough to use, but the closeout still
shows a meaningful fixed scan cost:

- no-op `graph index --json`: `17.71s`
- `graph status --json`: `0.48s`
- `graph search ... --json`: `0.29s`
- `graph context ... --json`: `1.58s`

The graph is now product-usable. This lane is about tightening the remaining
scan floor without introducing risky background infrastructure.

## Scope

- measure file-walk and scan-metadata cost directly
- reduce repeated scan work in `graph index` and related freshness paths
- explore safe metadata reuse for unchanged scan state
- re-measure no-op index and stale/status cost after the reductions

## Hard Boundaries

- no filesystem watcher or daemon
- no platform-specific scan backend
- no implicit stale assumptions based only on directory mtimes
- no broad cache that can silently miss new, changed, or deleted files
- no public JSON contract drift

## Ordered Follow-Up Lanes

1. [`018-file-walk-and-scan-metadata-baseline.md`](./018-file-walk-and-scan-metadata-baseline.md)
2. [`019-safe-scan-metadata-reuse.md`](./019-safe-scan-metadata-reuse.md)
3. [`020-scan-cost-closeout-proof.md`](./020-scan-cost-closeout-proof.md)

## Batch Cards

- [`950-open-graph-scan-cost-lane.md`](./batch-cards/950-open-graph-scan-cost-lane.md)
- [`951-baseline-file-walk-and-scan-cost.md`](./batch-cards/951-baseline-file-walk-and-scan-cost.md)
- [`952-reduce-repeated-scan-work.md`](./batch-cards/952-reduce-repeated-scan-work.md)
- [`953-tighten-stale-and-status-scan-cost.md`](./batch-cards/953-tighten-stale-and-status-scan-cost.md)
- [`954-close-graph-scan-cost-proof.md`](./batch-cards/954-close-graph-scan-cost-proof.md)

## Acceptance

- no-op indexing is measurably cheaper than the `g07.013` closeout
- scan-related savings are explained, not just observed
- no correctness regression appears in added/changed/deleted path detection

## Next Task

No active batch card remains in this lane.
