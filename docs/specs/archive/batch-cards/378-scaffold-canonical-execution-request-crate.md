# 378 - Scaffold Canonical Execution Request Crate

Lane: [`037-canonical-task-execution-request-and-pipeline-strict-lane.md`](../037-canonical-task-execution-request-and-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Create the first `effigy-execution` crate slice with the canonical request and
plan types needed by direct tasks, embedded callers, and future Rhai
`exec::run(...)`.

## Scope

- add `crates/effigy-execution`
- define `TaskExecutionRequest`
- define `TaskExecutionRequestBuilder`
- define `ResolvedTaskExecutionPlan`
- define execution enums for surface, intent, output mode, runtime policy,
  handoff policy, cleanup policy, and environment plan
- accept `EffigyRuntimeContext` as the context input
- add unit tests for host and container-intent request building
- do not wire runner execution yet
- do not expose Rhai `exec::run(...)` yet

## Exit Condition

This card is complete when the crate exists, builds independently, and can
produce stable host/container-intent plans from a captured runtime context.

## Closeout

`crates/effigy-execution` now exists. It defines:

- `TaskExecutionRequest`
- `TaskExecutionRequestBuilder`
- `ResolvedTaskExecutionPlan`
- `ExecutionSurface`
- `ExecutionIntent`
- `ExecutionOutputMode`
- `ExecutionRuntimePolicy`
- `ExecutionHandoffPolicy`
- `ExecutionCleanupPolicy`
- `ExecutionEnvironmentPlan`
- `ExecutionRoute`

The first route resolver supports host intent, container intent, either intent,
and local container handoff mode.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-execution-target cargo test -p effigy-execution -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next Task

Implement card `379`.
