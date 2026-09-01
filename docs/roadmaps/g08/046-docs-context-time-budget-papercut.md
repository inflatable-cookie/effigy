# g08.046 Docs Context Time-Budget Papercut

Status: Complete
Created: 2026-09-01
Completed: 2026-09-01
Card: [`1101`](./batch-cards/1101-bound-docs-context-cold-refresh.md)
Contract: [`041`](../../contracts/041-documentation-graph-profile-contract.md)
Papercut: [`PAPERCUTS.md`](../../../PAPERCUTS.md)
Evidence: [`../../logs/2026-09/01-184159-docs-context-time-budget-1101.md`](../../logs/2026-09/01-184159-docs-context-time-budget-1101.md)

## Purpose

Make a cold or stale `docs context` refresh visibly bounded by the same policy
and typed failure used by `effigy graph`.

## Decision

- `EFFIGY_GRAPH_TIMEOUT_MS` governs graph commands and lazy-refresh consumers.
- `0` disables the bound everywhere.
- cold/stale docs refresh announces progress on stderr before the repository walk.
- timeout detail, health snapshot, and recovery stay shared; stdout contracts do
  not change.

## Scope

- shared graph time-budget and bounded-operation seam
- docs-context command integration
- focused text and JSON recurrence proof

## Cards

- [x] [`1101`](./batch-cards/1101-bound-docs-context-cold-refresh.md) — complete

## Acceptance

- a forced slow cold refresh fails within the configured bound
- text and JSON retain valid stdout while progress stays on stderr
- warm/current queries do not claim a refresh
- graph-command timeout behavior remains unchanged

## Next Task

Return the exact-head PR to the Effigy orchestrator. Shared PAPERCUTS,
changelog, contract, and guide closeout stay with serial merge.
