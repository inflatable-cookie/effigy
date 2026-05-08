# 433 - Add Execution Dispatch Plan Foundation

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Add the first pure execution planning types to `effigy-execution` and make
`run_manifest_task_request` consume a resolved dispatch plan before falling
through to the existing runner pipeline.

## Scope

- add `ExecutionPreflightInput`
- add `ExecutionRuntimeArgsPlan`
- add `ExecutionPreflightPlan`
- add `ExecutionDispatchInput`
- add `ExecutionDispatchPlan`
- add `ExecutionPlanDiagnostic`
- add builder/helper methods needed to construct these from
  `TaskExecutionRequest`
- keep catalog discovery, task selection, binding, runtime prep, managed
  dispatch, standard dispatch, and process execution in runner for this card
- update `src/runner/execute/entry.rs` so `run_manifest_task_request` resolves
  the new dispatch plan before calling the existing preflight/pipeline path
- add focused crate tests proving equivalent request inputs produce equivalent
  dispatch plans

## Non-Goals

- no runtime activation migration
- no container manager migration
- no public CLI behavior changes
- no public JSON schema changes
- no release work
- no `.github/workflows/` edits

## Implementation Notes

`ExecutionDispatchPlan` should be intentionally conservative:

- carry the original `TaskExecutionRequest`
- carry normalized task selector and args
- carry effective cwd from `ExecutionEnvironmentPlan` or runtime context
- carry env override count and keys, but not inspect secret values
- carry output mode
- carry route from `ResolvedTaskExecutionPlan`
- carry surface

The first runner integration should not change behavior. It should replace the
manual request unpacking in `run_manifest_task_request` with use of the plan.

## Exit Condition

This card is complete when `run_manifest_task_request` consumes
`ExecutionDispatchPlan`, behavior is unchanged, and `effigy-execution` has
focused tests for direct, Rhai, bootstrap, and inside-container request shapes.

## Validation

- `cargo test -p effigy-execution`
- `cargo test -p effigy --lib execute`
- `git diff --check`

## Closeout

Added the first pure dispatch-plan types to `effigy-execution` and routed
`run_manifest_task_request` through `ExecutionDispatchPlan` before the existing
runner preflight and pipeline.

The dispatch plan now carries normalized selector and args, effective cwd,
surface, route, output mode, and env override keys without exposing env values.
Focused tests cover direct, bootstrap, Rhai, run-array, inside-container, and
non-task request shapes.

## Next Task

Start card
[`434-select-next-execution-planning-slice.md`](./434-select-next-execution-planning-slice.md).
