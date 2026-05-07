# 471 - Add Container Ops Lifecycle Plan Foundation

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Add the first dependency-light `effigy-container-ops` crate foundation for
container lifecycle operation planning.

## Scope

- add `crates/effigy-container-ops`
- define request, plan, kind, side-effect class, safety policy, and report
  types
- support lifecycle operations first:
  - `up`
  - `down`
  - `reset`
- keep the crate pure and side-effect free
- add focused unit tests for stable planning behavior

## Non-Goals

- no runner migration yet
- no backend calls
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `cargo test -p effigy-container-ops` passes and the
crate exposes a small lifecycle operation plan substrate.

## Closeout

Added `crates/effigy-container-ops` with pure lifecycle operation planning for:

- `up`
- `down`
- `reset`

The first plan shape carries operation identity, backend id, side-effect class,
confirmation policy, and report result without taking dependencies on runner,
CLI, or container backend code.

## Validation

- `cargo test -p effigy-container-ops`
- `git diff --check`

## Next Task

Wire lifecycle `container up/down/reset` planning into runner command glue.
