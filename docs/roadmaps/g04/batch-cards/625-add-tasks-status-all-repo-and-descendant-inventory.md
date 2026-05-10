# 625 - Add Tasks Status All Repo And Descendant Inventory

Lane: [`063-task-status-query-surface-and-read-model-strict-lane.md`](../063-task-status-query-surface-and-read-model-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Goal

Widen the new task-status read surface from one resolved selector to the
current repo-plus-descendant inventory view.

## Scope

- add `effigy tasks status --all`
- inventory declared tasks for the current repo root and descendants
- merge declared tasks with active/latest task-status records
- keep undeclared stale rows visible instead of dropping them
- render grouped text output and `effigy.tasks-status-all.v1` JSON output

## Non-Goals

- no machine-wide or `--global` status scope
- no control verbs
- no cleanup or retention policy
- no release work

## Exit Condition

This card is complete when Effigy can render repo-plus-descendant task status
inventory in text and JSON, including declared unknown rows and stale
no-longer-declared rows.

## Validation

- parser tests for `tasks status --all`
- repo inventory tests for declared, active, completed, unknown, and stale rows
- JSON contract tests for `effigy.tasks-status-all.v1`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Add the final docs/examples pass for the task-status query surface once
`--all` inventory is landed.
