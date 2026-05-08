# 380 - Migrate Direct Task Dispatch To Execution Request

Lane: [`037-canonical-task-execution-request-and-pipeline-strict-lane.md`](../037-canonical-task-execution-request-and-pipeline-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Make the direct CLI `Command::Task` dispatch path build a
`TaskExecutionRequest` before entering the existing execution pipeline.

## Scope

- add runner API support for `TaskExecutionRequest`
- keep the existing execution preflight and pipeline behavior unchanged
- make direct `Command::Task` dispatch build through `TaskExecutionRequestBuilder`
- preserve cwd, selector, args, output behavior, and error behavior
- add a focused regression test for direct task dispatch through the new request
  path

## Exit Condition

This card is complete when direct task dispatch has a request-builder boundary
and still produces the same output as the old path.

## Closeout

Direct `Command::Task` dispatch now builds a `TaskExecutionRequest` with
`ExecutionSurface::DirectCli` before entering the existing task pipeline.

The runner API also has `run_manifest_task_request(...)` as the compatibility
bridge from the request model into the old execution path.

## Validation

- targeted direct task runner test
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `CARGO_TARGET_DIR=/tmp/effigy-execution-target cargo test -p effigy-execution -- --nocapture`

## Next Task

Implement card `381`.
