# 434 - Select Next Execution Planning Slice

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Ready
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

## Validation

- docs path check for updated spec and roadmap front doors
- `git diff --check`

## Next Task

Create the next bounded execution pipeline implementation card.
