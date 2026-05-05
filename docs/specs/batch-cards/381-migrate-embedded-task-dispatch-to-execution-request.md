# 381 - Migrate Embedded Task Dispatch To Execution Request

Lane: [`037-canonical-task-execution-request-and-pipeline-strict-lane.md`](../037-canonical-task-execution-request-and-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Move embedded task dispatch through `TaskExecutionRequestBuilder`.

## Scope

- update `run_embedded_task(...)` to build `TaskExecutionRequest`
- use `ExecutionSurface::RunArray` for run-array task steps initially
- preserve current cwd/repo targeting behavior
- add a focused regression around run-array task dispatch
- do not migrate embedded builtin commands in this card

## Exit Condition

This card is complete when run-array task steps cross the execution request
boundary without changing output or repo-target behavior.

## Closeout

`run_embedded_task(...)` now builds a `TaskExecutionRequest` with
`ExecutionSurface::RunArray` before entering the existing task execution
pipeline.

## Validation

- targeted run-array task test
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next Task

Choose the next execution request migration card.
