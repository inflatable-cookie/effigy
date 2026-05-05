# 035 - Contract Promotion Public Cleanup Breaks And Closeout

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-05
Started: 2026-05-05
Completed: 2026-05-05
Depends on: [`034-dependability-proof-matrix-for-decodelabs-and-underlay-shapes.md`](./034-dependability-proof-matrix-for-decodelabs-and-underlay-shapes.md)

## Goal

Promote durable runtime/context/container/execution truth into contracts and
close the modularisation round cleanly.

## Scope

- update container runtime and execution convergence contracts
- add or update runtime context, container manager, and task execution request
  contracts
- update architecture package map
- document intentional cleanup breaks in `CHANGELOG.md`
- close front doors with no stale ready card

## Non-Goals

- release execution
- workflow edits

## Closeout

Closed with the modularisation contract set promoted:

- `005-container-runtime-contract.md` now names context, manager, and request
  ownership for container-backed local execution
- `009-execution-surface-convergence.md` now names the task request builder and
  Rhai `exec::run(...)` route
- `013-task-execution-request-contract.md` now owns the canonical request/plan
  surface
- `010-package-map.md` now names `effigy-context`,
  `effigy-container-manager`, and `effigy-execution`
- no new cleanup-break changelog entry was needed

## Next Task

Planning stop. Choose the next roadmap or request release work explicitly.
