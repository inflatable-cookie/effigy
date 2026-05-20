# 088 - Graph Watch Mode Strict Lane

Roadmap: [`g07.021`](../roadmaps/g07/021-graph-watch-mode-suite.md)
Related planning:
- [`g07.022`](../roadmaps/g07/022-watch-backend-and-debounce-rules.md)
- [`g07.023`](../roadmaps/g07/023-dirty-reconcile-and-overflow-fallback.md)
- [`g07.024`](../roadmaps/g07/024-graph-watch-closeout-proof.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Execute a bounded watcher lane that keeps the graph fresh through explicit
foreground watch mode rather than hidden background infrastructure.

## Lane Posture

Posture: `strict-closed`

This lane is executable because the graph is now cheap enough to refresh from
filesystem events:

- no-op `graph index --json`: `0.25s`
- `graph status --json`: `0.21s` to `0.24s`
- incremental change slices are already bounded

## Hard Boundaries

- no daemon, detach service, or PID registry
- no watcher dependency for graph correctness
- no event-only path that can silently miss deletes or overflow
- no public JSON contract drift outside the new watch surface

## Execution Order

1. `960` complete: watch lane opened
2. `961` complete: watch-mode baseline shape
3. `962` complete: implement watch backend and debounce
4. `963` complete: add overflow reconcile and surface proof
5. `964` complete: closeout proof

## Ready Chain

- `960` through `964` are complete
- no active ready card remains

## Auto-Continuation Envelope

Auto-start is enabled while:

- the previous card closes green
- watch-mode behavior remains bounded to the foreground command
- reconcile paths stay explicit and testable

Stop and replan if implementation discovers:

- the generic watcher backend is too inconsistent for a trustworthy default
- update latency requires hidden background state
- correctness depends on heuristics that cannot explain deletes or overflows

## Acceptance

This lane is complete when:

- watch mode works with the default debounce
- overflow/drop handling is explicit
- closeout evidence shows bounded update behavior
- no active watch card remains

## Next Task

No active task remains in lane `088`.
