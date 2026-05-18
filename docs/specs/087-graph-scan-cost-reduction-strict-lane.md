# 087 - Graph Scan Cost Reduction Strict Lane

Roadmap: [`g07.017`](../roadmaps/g07/017-graph-scan-cost-reduction-suite.md)
Related planning:
- [`g07.018`](../roadmaps/g07/018-file-walk-and-scan-metadata-baseline.md)
- [`g07.019`](../roadmaps/g07/019-safe-scan-metadata-reuse.md)
- [`g07.020`](../roadmaps/g07/020-scan-cost-closeout-proof.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Execute a bounded graph performance lane aimed only at file-walk and scan-cost
reduction after the extractor and query work already landed.

## Lane Posture

Posture: `strict-closed`

This lane is executable because the remaining scan floor is concrete and
measured:

- no-op `graph index --json`: `17.71s`
- `graph status --json`: `0.48s`
- extractor and failed-path work are already out of the way

## Hard Boundaries

- no watcher, daemon, or background scan service
- no platform-specific scan backend
- no stale path shortcuts based only on directory mtimes
- no correctness regressions in added/changed/deleted path detection
- no public JSON contract drift

## Execution Order

1. `950` complete: scan-cost lane opened
2. `951` complete: baseline file-walk and scan cost
3. `952` complete: reduce repeated scan work
4. `953` complete: tighten stale and status scan cost
5. `954` complete: closeout proof

## Ready Chain

- `950` through `954` are complete
- no active ready card remains in this lane

## Auto-Continuation Envelope

Auto-start is enabled while:

- the previous card closes green
- the next reduction still has direct measurements behind it
- path-detection correctness remains explicit and testable

Stop and replan if implementation discovers:

- the remaining cost is mostly outside Effigy control
- the next win requires background infra or unsafe heuristics
- command correctness depends on scan behavior that cannot be reduced cleanly

## Acceptance

This lane is complete when:

- the scan floor is measured directly
- no-op indexing is cheaper than the `g07.013` closeout
- status/stale scan costs are reduced or clearly explained
- residual limits are written down explicitly

## Next Task

No active task remains in lane `087`.
