# g08.047 Docs Context Traversal-Budget Papercut

Status: Complete
Created: 2026-09-01
Completed: 2026-09-01
Card: [`1102`](./batch-cards/1102-reserve-docs-context-traversal-slot.md)
Evidence: [`../../logs/2026-09/01-172541-docs-context-traversal-budget-1102.md`](../../logs/2026-09/01-172541-docs-context-traversal-budget-1102.md)
Contract: [`041`](../../contracts/041-documentation-graph-profile-contract.md)
Papercut: [`PAPERCUTS.md`](../../../PAPERCUTS.md)

## Purpose

Make typed-relation traversal reachable when a large lexical seed set would
otherwise consume the complete section budget.

## Decision

- With at least two section slots, keep the best lexical result and reserve one
  slot for the best whole traversed result that fits the byte budget.
- Fill remaining capacity using the existing deterministic ranking.
- A one-section query keeps the best lexical result.
- No second query mode or ranking implementation is introduced.

## Scope

- documentation-context candidate selection and budgeting
- deterministic large-corpus fixture proof
- existing benchmark regression validation

## Cards

- [x] [`1102`](./batch-cards/1102-reserve-docs-context-traversal-slot.md) — complete

## Acceptance

- a relation result survives lexical saturation with `max-sections >= 2`
- direct lexical evidence remains first
- one-slot, no-traversal, and oversized-traversal cases stay deterministic
- provenance, relevance gates, truncation, and current benchmark behavior hold

## Next Task

Return the exact-head PR to the Effigy orchestrator. Shared PAPERCUTS,
changelog, contract, and guide closeout stay with serial merge.
