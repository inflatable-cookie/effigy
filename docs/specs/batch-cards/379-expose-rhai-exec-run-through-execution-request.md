# 379 - Expose Rhai Exec Run Through Execution Request

Lane: [`037-canonical-task-execution-request-and-pipeline-strict-lane.md`](../037-canonical-task-execution-request-and-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Add the first `exec::run(command, options)` Rhai helper backed by
`TaskExecutionRequestBuilder`.

## Scope

- add `exec` to the Rhai module registry
- implement capture-mode `exec::run(command, options)`
- parse `run_in`, `container`, `service`, `stdin_file`, `cwd`, and `env`
- build a `TaskExecutionRequest` with `ExecutionSurface::Rhai`
- route host intent to host process execution
- route container intent to existing container exec callbacks
- use direct host execution for local container handoff routes
- return process-like output plus route detail
- keep stream/tee/interactive output modes out of scope

## Exit Condition

This card is complete when Rhai can express the DecodeLabs mysql seed shape with
`run_in = "container"`, `service = "db"`, and `stdin_file`, and tests prove the
helper builds through `effigy-execution`.

## Closeout

Rhai now exposes capture-mode `exec::run(command, options)`.

Implemented options:

- `run_in`
- `container`
- `service`
- `stdin_file`
- `cwd`
- `env`

The helper builds `TaskExecutionRequest` with `ExecutionSurface::Rhai`, returns
process-like output, and includes route detail. Host routes use host process
execution. Container routes use the existing container exec callback. Local
container handoff routes execute directly on the current process side.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-rhai exec -- --nocapture`
- targeted runner Rhai test for `exec::run(...)`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next Task

Create the next execution request migration card.
