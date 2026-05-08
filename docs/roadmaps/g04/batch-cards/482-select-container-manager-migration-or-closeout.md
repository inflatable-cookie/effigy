# 482 - Select Container Manager Migration Or Closeout

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Decide whether `g04.004` should continue into backend-manager execution
migration now or close with the typed operation planning substrate complete.

## Scope

- review `g04.004` acceptance criteria
- inventory remaining direct compose/backend calls in container operation paths
- choose one next move:
  - start manager-backed lifecycle/read/exec/data migration
  - add operation-report plumbing first
  - close `g04.004` and hand off the remaining direct-call cleanup to later
    milestones
- update lane and roadmap front doors
- do not implement code in this decision card

## Non-Goals

- no public CLI behavior changes
- no code migration
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when one next implementation or closeout card is ready.

## Closeout

`g04.004` should continue into backend-manager migration.

The typed operation taxonomy now covers lifecycle, read, exec/shell, data, and
cache operations, but direct compose/runtime calls still remain in runtime and
container command paths. Closing now would leave the milestone short of its
core acceptance condition: runner/runtime container operations must stop owning
backend command construction.

The next slice is a small manager substrate step, not a broad caller rewrite:
add a manager-owned compose invocation plan that existing lifecycle/read/data
paths can consume in later cards.

## Validation

- `git diff --check`

## Next Task

Add the manager-backed compose invocation plan foundation.
