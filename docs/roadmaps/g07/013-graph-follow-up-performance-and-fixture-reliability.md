# g07.013 - Graph Follow-Up Performance And Fixture Reliability

Status: Complete
Depends on: `g07.001` through `g07.012`

## Goal

Land the first post-launch hardening tranche for the native code graph surface.

This lane exists because the closeout evidence was good enough to ship the
feature, but still showed three concrete gaps:

- no-op indexing is still effectively full reindex cost
- query latency is still too high for small/common lookups
- full-repo indexing still reports seven known failed fixture or bundle paths

## Scope

- add a real incremental/no-op index short path
- reduce query and context-pack latency without weakening the JSON contracts
- fix the seven failed full-repo fixture and bundle paths from the `g07`
  closeout
- refresh the measured proof after the fixes land

## Ordered Follow-Up Lanes

1. [`014-incremental-indexing-and-cache-reuse.md`](./014-incremental-indexing-and-cache-reuse.md)
2. [`015-query-speed-and-projection-reduction.md`](./015-query-speed-and-projection-reduction.md)
3. [`016-failed-graph-fixture-path-reliability.md`](./016-failed-graph-fixture-path-reliability.md)

## Hard Boundaries

- no MCP, daemon, or plugin runtime
- no DB-layout breakage as a casual optimization
- no weakening of provenance, confidence, or JSON schema guarantees
- no hiding failed paths to improve metrics
- no query caching that can silently return stale results without explicit
  freshness evidence

## Acceptance Criteria

- no-op indexing is materially cheaper than cold full-repo indexing
- query improvements are measured against the `g07` closeout baseline
- the seven known failed graph paths are either fixed or explicitly reclassified
  with a documented reason
- the lane ends with a new measured closeout log, not a qualitative claim

## Batch Cards

- [`930-open-graph-follow-up-lane.md`](./batch-cards/930-open-graph-follow-up-lane.md)
- [`931-baseline-incremental-query-and-failed-path-inventory.md`](./batch-cards/931-baseline-incremental-query-and-failed-path-inventory.md)
- [`932-implement-incremental-index-short-path.md`](./batch-cards/932-implement-incremental-index-short-path.md)
- [`933-reduce-query-latency-and-context-projection-cost.md`](./batch-cards/933-reduce-query-latency-and-context-projection-cost.md)
- [`934-fix-failed-graph-fixture-path-indexing.md`](./batch-cards/934-fix-failed-graph-fixture-path-indexing.md)
- [`935-close-graph-follow-up-proof.md`](./batch-cards/935-close-graph-follow-up-proof.md)

## Next Task

No active batch card remains in this lane.
