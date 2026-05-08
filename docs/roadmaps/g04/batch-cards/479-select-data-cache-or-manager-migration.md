# 479 - Select Data Cache Or Manager Migration

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next `g04.004` slice after lifecycle, read, and exec/shell operation
plans are wired.

## Scope

- review remaining work:
  - data/cache operation planning
  - manager-backed lifecycle/read/exec execution migration
  - operation report promotion
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

- model data/cache operations next

Rationale:

- lifecycle, read, and exec/shell operations already have typed plans
- data/cache completes the operation taxonomy before backend-manager migration
- manager-backed execution should consume one complete operation model rather
  than grow more one-off adapter shapes

## Validation

- `git diff --check`

## Next Task

Add data/cache operation plans.
