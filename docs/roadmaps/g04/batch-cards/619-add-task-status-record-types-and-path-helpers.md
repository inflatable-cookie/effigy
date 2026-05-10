# 619 - Add Task Status Record Types And Path Helpers

Lane: [`062-task-status-record-and-active-run-model-strict-lane.md`](../062-task-status-record-and-active-run-model-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-10

## Goal

Add the typed task-status key/record model and the canonical runtime/report path
helpers before any execution-hook wiring begins.

## Scope

- add `TaskStatusKey`
- add typed status state/stage enums
- add active and completed record structs or variants under one shared model
- add deterministic key derivation from normalized task identity
- add runtime/report path helpers for:
  - active record path
  - latest completed record path
  - history record path
- keep the result purely structural; no shared writer hooks yet

## Non-Goals

- no execution-pipeline hook wiring yet
- no stale-record cleanup command
- no final `effigy tasks status` read/query surface
- no machine-wide inventory
- no release work

## Exit Condition

This card is complete when the codebase has one typed task-status model and one
canonical set of storage-path helpers that later writer and reader work can
share without reopening identity or file-layout decisions.

## Validation

- focused unit tests for deterministic key derivation
- path-helper tests for active/latest/history locations
- descendant-scope collision tests
- `cargo fmt --all -- --check`
- `git diff --check`

## Closeout

- added a typed task-status identity, key, state, stage, warning, outcome, and
  active/completed record model in `effigy-execution`
- added deterministic key derivation with descendant-scope collision coverage
- added canonical runtime/report path helpers in `effigy-runtime` for active,
  latest, and history record locations

## Next Task

Card
[`620-wire-task-status-writer-into-canonical-execution-path.md`](./620-wire-task-status-writer-into-canonical-execution-path.md).
