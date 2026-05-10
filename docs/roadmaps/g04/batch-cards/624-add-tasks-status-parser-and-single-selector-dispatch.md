# 624 - Add Tasks Status Parser And Single-Selector Dispatch

Lane: [`063-task-status-query-surface-and-read-model-strict-lane.md`](../063-task-status-query-surface-and-read-model-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-10

## Goal

Add the first user-facing `tasks status` parser/dispatch slice for one resolved
selector on top of the completed task-status read model.

## Scope

- add `effigy tasks status <selector>`
- add `--json`
- route the selector through normal task resolution
- load task-status reconciliation results for the resolved selector
- render one-selector text and JSON results
- keep `--all` inventory for the next card

## Non-Goals

- no `tasks status --all` inventory yet
- no machine-wide status scope
- no control verbs
- no cleanup or retention policy
- no release work

## Exit Condition

This card is complete when Effigy can answer task status for one resolved task
selector in text and JSON form using the shared reconciliation layer.

## Validation

- parser tests for `tasks status <selector>`
- single-selector status tests for active, completed, missing, and ambiguous
  routing cases
- JSON contract tests for `effigy.tasks-status.v1`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Execute
[`625-add-tasks-status-all-repo-and-descendant-inventory.md`](./625-add-tasks-status-all-repo-and-descendant-inventory.md)
to widen the surface to repo-plus-descendant inventory.
