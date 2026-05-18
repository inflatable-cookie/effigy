# 952 - Reduce Repeated Scan Work

Roadmap: [`../019-safe-scan-metadata-reuse.md`](../019-safe-scan-metadata-reuse.md)
Strict lane: [`../../../specs/087-graph-scan-cost-reduction-strict-lane.md`](../../../specs/087-graph-scan-cost-reduction-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Remove the first duplicated scan passes from the no-op graph index path.

## Scope

- reduce repeated repo walks in `graph index`
- keep added/changed/deleted detection explicit
- add focused regression proof

## Acceptance

- no-op index timing improves measurably without path-detection regressions

## Results

- removed the second repo walk from the no-op `graph index` path
- reused the existing scan entries for stale-path calculation
- measured no-op `graph index --json` improvement: `0.39s -> 0.32s`

## Next Task

Execute `953`.
