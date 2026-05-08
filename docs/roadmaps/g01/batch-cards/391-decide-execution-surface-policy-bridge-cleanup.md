# 391 - Decide Execution Surface Policy Bridge Cleanup

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Decide how to reduce duplicated execution-surface policy glue between
`effigy-execution` and runner runtime-prep code.

## Scope

- inspect `container_runtime_prep::ExecutionSurfaceKind`
- inspect standard, deferral, bootstrap, and explicit exec activation callers
- decide whether to:
  - map `ExecutionSurface` directly into runtime prep
  - keep a narrower runtime-prep-only enum
  - or move activation policy into `effigy-execution`
- choose a first implementation card
- no behavior changes

## Exit Condition

This card is complete when the execution-surface bridge boundary is explicit
and the next implementation card has a narrow write set.

## Decision

Remove the runner-only `ExecutionSurfaceKind` field from
`container_runtime_prep::ActivationRequest`.

Reasoning:

- `ExecutionSurfaceKind` is narrower than `effigy_execution::ExecutionSurface`
- the field has no production behavior in runtime prep
- tests only used it as a label while proving equal behavior across standard,
  deferred, and explicit exec surfaces
- mapping it to `ExecutionSurface` would make runtime prep depend on a broader
  surface enum without adding behavior
- moving activation policy into `effigy-execution` is premature while runtime
  prep still owns host lease and gateway side effects

## Next Boundary

Keep runtime prep as the owner of activation side effects for now.
`effigy-execution` remains the request/plan owner.

The first cleanup should delete the unused bridge field and simplify tests.

## Next Task

Implement card `392`: remove unused `ExecutionSurfaceKind` from runtime prep.
