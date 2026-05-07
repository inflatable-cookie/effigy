# 434 - Select Next Execution Planning Slice

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next `g04.002` implementation slice after the dispatch-plan
foundation.

## Scope

- review the new `ExecutionDispatchPlan` boundary
- decide whether the next slice should migrate runner preflight inputs,
  selection/binding planning, or embedded dispatch wrappers
- define the smallest implementation card that moves ownership into
  `effigy-execution` without changing public behavior
- keep side-effectful runtime/container execution in runner until pure plans
  are stable

## Non-Goals

- no runtime activation migration
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the next bounded implementation card is ready and
the lane/front-door docs point to it.

## Decision

Select preflight input migration as the next slice.

The dispatch-plan foundation is the right seam: it already normalizes selector,
args, cwd, surface, output mode, route, and env override keys before runner
preflight starts. The next awkwardness is that runner preflight still accepts a
`TaskInvocation` plus cwd and then re-derives runtime args, JSON mode, selector
input, and repo override locally.

Do not move selection or binding next. Those paths still depend on runner-owned
catalog discovery and manifest lifetimes, so moving them before preflight input
would create a wide dependency move.

Do not spend the next slice on embedded wrappers. Direct CLI, bootstrap, Rhai,
run-array, demo, data seed, and deferral already have request-builder coverage
from `g03.032`; any remaining wrapper cleanup should happen after preflight
ownership is less local.

## Next Implementation Slice

Card `435` should make runner preflight consume
`ExecutionDispatchPlan`/`ExecutionPreflightInput` instead of a fresh
`TaskInvocation` plus cwd pair.

The card should be narrow:

- add helper constructors on `ExecutionPreflightInput`
- add a runtime-args planning helper that returns `ExecutionRuntimeArgsPlan`
- make `src/runner/execute/entry.rs` call preflight from the dispatch plan
- keep discovery, catalog loading, selection, binding, and pipeline dispatch in
  runner
- preserve CLI behavior and error text unless the existing error already comes
  from the shared planning helper

Expected follow-on after `435`: move pure discovery output shape into
`effigy-execution` or introduce a `SelectionInput` plan, depending on how much
runner-owned catalog lifetime code remains.

## Validation

- docs path check for updated spec and roadmap front doors
- `git diff --check`

## Closeout

Selected preflight input ownership as the next implementation step and created
card `435`.

## Next Task

Start card
[`435-move-execution-preflight-input-behind-dispatch-plan.md`](./435-move-execution-preflight-input-behind-dispatch-plan.md).
