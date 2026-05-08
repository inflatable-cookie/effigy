# 352 Implement Typed Gateway Loopback And Runtime Target Translation Errors

Status: archived
Updated: 2026-05-02
Roadmap: `g03.016`
Spec: `docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md`

## Objective

Land the gateway closeout slice for typed loopback allocation and runtime-target
translation failures in the runtime/container core.

## In Scope

- introduce the next explicit error-family split for high-signal gateway
  failures around:
  - loopback registry load/save/allocation
  - runtime target validation against running container rows
  - host-listener conflict translation
  - remaining route target/port selection seams that still flatten into generic
    invocation strings
- add focused category-level tests for the newly typed gateway closeout seams

## Out Of Scope

- full gateway-command taxonomy cleanup outside container runtime reconciliation
- broad wording polish across unrelated commands
- architecture authority repair

## Acceptance Criteria

- the remaining dominant gateway reconciliation seams inside the
  runtime/container core no longer rely on generic `task_invocation` strings
- tests assert on gateway error category rather than only rendered string output
- the lane is in a position for an honest closeout decision

## Validation

- targeted gateway loopback and runtime-target error tests
- `./target/debug/effigy docs check-paths docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md docs/roadmaps/g03/batch-cards/352-implement-typed-gateway-loopback-and-runtime-target-translation-errors.md docs/roadmaps/g03/batch-cards/353-decide-post-gateway-closeout-error-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/016-container-and-runtime-error-taxonomy-and-diagnostics.md`

## Next Task

Closed. Execute `353`.
