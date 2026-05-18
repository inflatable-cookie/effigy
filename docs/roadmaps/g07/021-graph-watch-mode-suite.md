# g07.021 - Graph Watch Mode Suite

Status: Complete
Depends on: `g07.001` through `g07.020`

## Goal

Add an explicit foreground watch mode that keeps the local graph index fresh
enough for agent work without turning the graph surface into a daemon-backed
subsystem.

## Why This Exists

The graph is now fast enough to make event-driven refresh practical:

- no-op `graph index --json`: `0.25s`
- `graph status --json`: `0.21s` to `0.24s`

That opens a simple watch loop:

- watch the repo
- debounce changes
- run incremental `graph index`
- surface fresh/stale state clearly

## Scope

- add `effigy graph watch`
- use a Rust filesystem watcher backend with a conservative debounce
- batch events into one incremental refresh pass
- recover cleanly from overflow, backend drops, and noisy event bursts
- prove the surface with text and JSON mode output

## Ordered Follow-Up Lanes

1. [`022-watch-backend-and-debounce-rules.md`](./022-watch-backend-and-debounce-rules.md)
2. [`023-dirty-reconcile-and-overflow-fallback.md`](./023-dirty-reconcile-and-overflow-fallback.md)
3. [`024-graph-watch-closeout-proof.md`](./024-graph-watch-closeout-proof.md)

## Hard Boundaries

- no background daemon, PID registry, or hidden service
- no watcher dependency for graph correctness
- no public query contract drift as a side effect of watch mode
- no event-only indexing path that can silently miss deletes or overflows
- no platform-specific special casing unless the generic backend proves broken

## Acceptance Criteria

- `graph watch` works as a foreground command with a default `1s` debounce
- changed files refresh the graph through the existing incremental index path
- overflow/drop paths reconcile explicitly instead of guessing
- closeout evidence proves watch mode updates are bounded and predictable

## Batch Cards

- [`960-open-graph-watch-lane.md`](./batch-cards/960-open-graph-watch-lane.md)
- [`961-baseline-watch-mode-shape.md`](./batch-cards/961-baseline-watch-mode-shape.md)
- [`962-implement-watch-backend-and-debounce.md`](./batch-cards/962-implement-watch-backend-and-debounce.md)
- [`963-add-overflow-reconcile-and-surface-proof.md`](./batch-cards/963-add-overflow-reconcile-and-surface-proof.md)
- [`964-close-graph-watch-proof.md`](./batch-cards/964-close-graph-watch-proof.md)

## Next Task

No active batch card remains in this lane.
