# 473 - Select Next Container Operation Family

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next container operation family to model after lifecycle planning.

## Scope

- review current lifecycle plan wiring
- choose one next implementation card:
  - exec/shell operations
  - read-only status/logs/stats operations
  - data/cache operations
  - lifecycle backend-manager migration
- update the lane and roadmap front doors
- do not implement code in this decision card

## Non-Goals

- no public CLI behavior changes
- no code migration
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when one next operation-family implementation card is
ready.

## Closeout

Decision:

- model read-only status/logs/stats operations next

Rationale:

- these operations are non-destructive and should not affect runtime state
- they already have clear report surfaces
- modeling them before exec/shell/data reduces risk while widening operation
  identity beyond lifecycle

## Validation

- `git diff --check`

## Next Task

Add read-only status/logs/stats operation plans.
