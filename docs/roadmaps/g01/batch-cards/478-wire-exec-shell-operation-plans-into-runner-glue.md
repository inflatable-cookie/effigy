# 478 - Wire Exec Shell Operation Plans Into Runner Glue

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make container exec and shell runner paths build typed exec/shell operation
plans before side effects run.

## Scope

- wire plans into `run_container_shell`
- wire plans into `run_container_exec_capture_with_options`
- preserve current command construction and backend execution
- preserve Rhai/DB seed stdin-file behavior
- add focused plan identity tests in runner glue

## Non-Goals

- no backend-manager migration yet
- no data/cache migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when exec/shell runner paths construct typed operation
plans and focused container command tests pass.

## Closeout

Runner glue now constructs typed exec/shell operation plans for:

- `container shell`
- captured container exec
- stdin-file container exec used by Rhai and data flows

The existing compose command construction and backend execution paths are
unchanged.

## Validation

- `cargo test -p effigy-container-ops`
- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Select data/cache or backend-manager migration as the next operation slice.
