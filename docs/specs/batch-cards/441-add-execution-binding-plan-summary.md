# 441 - Add Execution Binding Plan Summary

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Add lifetime-light binding input/result summary types to `effigy-execution`
without moving runner-owned container binding behavior.

## Scope

- add `ExecutionBindingInput`
- add `ExecutionBindingPlan`
- add a shared binding kind enum
- make runner binding resolution build the shared summary after resolving
  `ExecutionBindingResolution`
- keep `ContainerExecutionBinding` and `ExecutionBindingResolution` in runner
  for actual dispatch
- preserve inline workspace support errors and existing routing behavior
- add focused tests for host, none, named container, and inline container
  summaries where existing fixtures make that cheap

## Non-Goals

- no `ManifestTask` ownership migration
- no `ContainerExecutionBinding` ownership migration
- no container policy loading migration
- no runtime activation migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when successful runner binding resolution produces a
shared binding plan summary and existing standard/managed dispatch still uses
runner-owned binding values.

## Validation

- `cargo test -p effigy-execution`
- `cargo test -p effigy --lib execute`
- `git diff --check`

## Closeout

Added lifetime-light binding input and plan summary types to
`effigy-execution`. Runner binding resolution now builds an
`ExecutionBindingPlan` for standard and managed task paths while preserving
runner-owned `ContainerExecutionBinding` and `ExecutionBindingResolution` for
actual dispatch, policy loading, and inline workspace behavior.

No public CLI behavior changed.

## Next Task

Start card
[`442-select-dispatch-stage-or-runtime-activation-handoff.md`](./442-select-dispatch-stage-or-runtime-activation-handoff.md).
