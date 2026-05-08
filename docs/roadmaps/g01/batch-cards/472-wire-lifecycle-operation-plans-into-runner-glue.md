# 472 - Wire Lifecycle Operation Plans Into Runner Glue

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make `container up`, `container down`, and `container reset` build
`effigy-container-ops` lifecycle plans before side effects run.

## Scope

- wire lifecycle operation plans into runner lifecycle command glue
- preserve current CLI behavior and JSON rendering
- keep actual Docker/Colima execution in existing adapters for this card
- use operation plans for safety/confirmation decisions where it is a direct
  mapping
- add focused tests around plan identity and reset safety policy

## Non-Goals

- no backend manager migration yet
- no data/shell/exec/log/status/cache migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when lifecycle runner code constructs typed operation
plans and existing container lifecycle tests remain stable.

## Closeout

Lifecycle runner glue now builds `effigy-container-ops` plans for:

- `container up`
- `container down`
- `container reset`

The existing backend execution path remains unchanged. Reset wipe-data
confirmation now reads from the typed plan confirmation policy while preserving
the current prompt text and CLI behavior.

## Validation

- `cargo test -p effigy-container-ops`
- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Select the next container operation family: exec/shell or read-only status/logs.
