# 432 - Scaffold Execution Pipeline Ownership Lane

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Open the implementation lane for `g04.002` and define the first safe execution
pipeline ownership slice.

## Scope

- create `docs/specs/044-execution-pipeline-ownership-strict-lane.md`
- inventory the current direct, bootstrap, Rhai, run-array, demo, deferral, and
  managed execution request paths
- define the first `effigy-execution` planning types to add
- decide which pure preflight data can move first without changing side effects
- create the first implementation card for the selected slice

## Non-Goals

- no implementation code changes in this scaffold card
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `g04.002` has a strict lane, a first implementation
card, and a bounded migration order that does not require guessing.

## Inventory

Current request-backed paths:

- direct CLI `Command::Task` builds `TaskExecutionRequestBuilder` in
  `src/runner/entrypoints/dispatch.rs`
- Rhai `exec::run(...)` builds `TaskExecutionRequestBuilder` in
  `crates/effigy-rhai/src/host_api.rs`
- helper callers use `run_manifest_task_with_surface*`, which builds a request
  and immediately delegates to `run_manifest_task_request`

Current wrapper-backed paths:

- Rhai `task::run(...)` callback uses `run_manifest_task_with_surface`
- Rhai command re-entry uses embedded command replay, then typed command
  dispatch
- run-array builtin re-entry uses `embedded_runner.rs`, then
  `run_manifest_task_with_surface`
- demo task execution uses `run_manifest_task_with_surface`
- DB seed uses `run_manifest_task_with_surface_and_env`
- doctor/builtin ports use `run_manifest_task_with_surface`
- bootstrap managed-run synthesis still builds a synthetic task selection in
  `run_managed_run_with_cwd`
- deferral still calls `build_execution_preflight` directly

Current runner-owned execution plan data:

- `ExecutionPreflight` in `src/runner/execute/preflight/context.rs`
- runtime args parsing and JSON stripping in
  `src/runner/execute/preflight/runtime.rs`
- selection in `src/runner/execute/selection.rs`
- binding in `src/runner/execute/binding.rs`
- standard dispatch in `src/runner/execute/pipeline/standard.rs`
- managed dispatch in `src/runner/execute/pipeline/managed.rs`

## First Slice Decision

The first implementation slice should add pure planning types to
`effigy-execution` without moving side effects.

Add:

- `ExecutionPreflightInput`
- `ExecutionRuntimeArgsPlan`
- `ExecutionPreflightPlan`
- `ExecutionDispatchInput`
- `ExecutionDispatchPlan`
- `ExecutionPlanDiagnostic`

Initial ownership:

- `effigy-execution` owns data shape and route/request consistency.
- runner keeps catalog discovery, selection, binding, and process/container
  side effects for now.
- `run_manifest_task_request` should resolve to an `ExecutionDispatchPlan`
  before it calls the existing runner pipeline.

First implementation card:

- `433-add-execution-dispatch-plan-foundation.md`

## Closeout

- strict lane `044` exists.
- execution request path inventory is captured.
- first implementation slice is selected.
- card `433` is ready.

## Next Task

Card
[`433-add-execution-dispatch-plan-foundation.md`](./433-add-execution-dispatch-plan-foundation.md).
