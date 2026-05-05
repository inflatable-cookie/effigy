# 391 - Decide Execution Surface Policy Bridge Cleanup

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

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

## Next Task

Choose the execution-surface policy cleanup card.
