# 086 - Graph Follow-Up Performance And Fixture Reliability Strict Lane

Roadmap: [`g07.013`](../roadmaps/g07/013-graph-follow-up-performance-and-fixture-reliability.md)
Related planning:
- [`g07.014`](../roadmaps/g07/014-incremental-indexing-and-cache-reuse.md)
- [`g07.015`](../roadmaps/g07/015-query-speed-and-projection-reduction.md)
- [`g07.016`](../roadmaps/g07/016-failed-graph-fixture-path-reliability.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Execute the first post-launch graph hardening tranche against the measured
closeout limits from `g07.012`.

## Lane Posture

Posture: `strict-closed`

This lane is executable because the target gaps are concrete and already
measured:

- no-op indexing cost
- query latency
- seven known failed full-repo fixture paths

## Hard Boundaries

- no MCP, daemon, or plugin runtime
- no DB-contract breakage as a shortcut
- no hidden failed-path suppression
- no stale implicit cache reuse
- no removal of provenance, confidence, reasons, or overflow evidence

## Execution Order

1. `930` complete: follow-up lane opened
2. `931` complete: baseline incremental/query/failed-path inventory
3. `932` complete: incremental index short path
4. `933` complete: query speed and projection reduction
5. `934` complete: failed fixture-path indexing fixes
6. `935` complete: closeout proof

## Ready Chain

- `930` through `935` are complete
- no active ready card remains in this lane

## Auto-Continuation Envelope

Auto-start is enabled while:

- the previous card closes green
- each optimization remains measurable
- failed paths stay explicit
- no public JSON contract drift is introduced accidentally

Stop and replan if implementation discovers:

- incremental indexing needs a schema reset beyond a bounded migration
- query speed requires a public contract compromise
- failed paths are actually unsupported product scope rather than bugs

## Acceptance

This lane is complete when:

- no-op indexing is materially cheaper than the original baseline
- query latency is measurably reduced
- the seven failed full-repo graph paths are fixed or deliberately
  reclassified with proof
- closeout evidence records the delta from `g07.012`

## Next Task

No active task remains in lane `086`.
