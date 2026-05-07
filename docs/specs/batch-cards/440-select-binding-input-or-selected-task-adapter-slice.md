# 440 - Select Binding Input or Selected Task Adapter Slice

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose whether the next `g04.002` slice can plan binding input from the
selection summary or needs a selected-task adapter first.

## Scope

- review `ExecutionSelectionPlan` and runner `TaskSelection<'a>` use
- inspect `resolve_container_execution_binding` inputs and ownership
- decide whether binding can consume lifetime-light task metadata now
- create the next smallest implementation card

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

Move binding input/plan summaries next.

Do not add a selected-task adapter yet. The selected `ManifestTask` still owns
the command body, env, managed run options, run-array details, workspace
binding, and run-in behavior needed by standard and managed dispatch. Cloning or
mirroring that shape in `effigy-execution` would create a second task model
before the dispatch pipeline is ready to consume it.

Binding is a better boundary:

- runner can still resolve binding from borrowed `TaskSelection<'a>`
- `effigy-execution` can own a lifetime-light `ExecutionBindingInput`
- runner can convert `ExecutionBindingResolution` into an
  `ExecutionBindingPlan` summary
- standard and managed dispatch can continue using runner-owned
  `ContainerExecutionBinding`

This keeps the new planning surface inspectable without moving container policy
loading or workspace/inline behavior yet.

## Next Implementation Slice

Card `441` should add binding input/result summary types and wire them into
runner binding resolution.

The implementation should summarize:

- selected task name
- catalog alias/root/manifest path/depth
- binding kind
- requested container name
- runtime surface
- whether inline workspace was requested

Expected follow-on after `441`: decide whether standard and managed dispatch can
consume a shared dispatch-stage input, or whether runtime activation planning
should start in `g04.003` before further execution-pipeline shrinkage.

## Validation

- docs path check for updated spec and roadmap front doors
- `git diff --check`

## Closeout

Selected binding input/plan summaries as the next implementation slice and
created card `441`.

## Next Task

Start card
[`441-add-execution-binding-plan-summary.md`](./441-add-execution-binding-plan-summary.md).
