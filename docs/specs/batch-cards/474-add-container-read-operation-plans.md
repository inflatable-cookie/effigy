# 474 - Add Container Read Operation Plans

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Extend `effigy-container-ops` with read-only operation plans for status, logs,
and stats.

## Scope

- add read operation variants to `ContainerOperationKind`
- support:
  - `status`
  - `logs`
  - `stats`
- model read-only side-effect class
- keep confirmation policy as no-confirmation
- add pure planning tests

## Non-Goals

- no runner migration yet
- no backend-manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `effigy-container-ops` exposes read-only operation
plans and focused tests pass.

## Closeout

Added read-only operation planning for:

- `status`
- `logs`
- `stats`

These operations now carry a `ReadsRuntime` side-effect class and no-confirm
safety policy in the shared operation model.

## Validation

- `cargo test -p effigy-container-ops`
- `git diff --check`

## Next Task

Wire status/logs/stats operation plans into runner/runtime glue.
