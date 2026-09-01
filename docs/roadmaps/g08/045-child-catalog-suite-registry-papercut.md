# g08.045 Child-Catalog Suite Registry Papercut

Status: Complete
Created: 2026-09-01
Card: [`1100`](./batch-cards/1100-preserve-ancestor-container-registry.md)
Contract: [`038`](../../contracts/038-unified-test-orchestration-contract.md)
Papercut: [`PAPERCUTS.md`](../../../PAPERCUTS.md)

## Purpose

Keep the originating repository's ancestor container registry available when a
test suite expands a task reference in a child catalog.

## Decision

- The selected catalog supplies the task cwd.
- The already loaded repository catalog graph and ancestor execution registry
  remain authoritative fallbacks.
- A child catalog's explicit execution registry overrides the ancestor.
- Direct child invocation keeps normal repository discovery semantics.
- Acowtancy's workaround remains until its owner revalidates downstream.

## Scope

- test suite task-reference planning and expansion
- focused synthetic parent/child catalog recurrence proof
- no Acowtancy edits

## Cards

- [x] [`1100`](./batch-cards/1100-preserve-ancestor-container-registry.md) — complete

## Acceptance

- child task refs retain an ancestor `[containers]` default
- child explicit configuration wins
- cwd, plan rendering, direct invocation, and unrelated suite forms do not drift
- focused tests and full Effigy QA pass

## Next Task

Return the exact-head PR for card `1100` to the Effigy orchestrator. Downstream
Acowtancy revalidation remains separately owned.
