# 476 - Select Next Container Operation Slice

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next `g04.004` operation slice after lifecycle and read operations
are represented as typed plans.

## Scope

- review remaining operation families:
  - exec/shell
  - data/cache
  - lifecycle/read backend-manager migration
- choose one next implementation card
- update lane and roadmap front doors
- do not implement code in this decision card

## Non-Goals

- no public CLI behavior changes
- no code migration
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when one next implementation card is ready.

## Closeout

Decision:

- model exec/shell operations next

Rationale:

- exec/shell is the highest-value remaining runtime-sensitive operation family
- Rhai and DB/data flows already depend on container exec behavior
- adding typed plans first lets the later manager migration happen without
  changing command semantics in the same slice

## Validation

- `git diff --check`

## Next Task

Add exec/shell operation plans.
