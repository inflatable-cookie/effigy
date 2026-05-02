# 350 Implement Typed Gateway Reconciliation And Route Translation Errors

Status: complete
Updated: 2026-05-02
Roadmap: `g03.016`
Spec: `docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md`

## Objective

Land the next bounded typed error slice for gateway reconciliation and route
translation in the container runtime path.

## In Scope

- introduce the next explicit error-family split for high-signal gateway and
  route reconciliation failures
- center the batch on:
  - route-table load/save translation
  - route register/deregister translation
  - one or two high-signal route-shape validation failures that still collapse
    into generic invocation strings
  - gateway reconciliation failures that sit directly on the runtime/container
    prep and workspace handoff path
- add focused category-level tests for the newly typed gateway seams

## Out Of Scope

- full gateway-command taxonomy cleanup
- broad wording polish across unrelated commands
- architecture authority repair

## Acceptance Criteria

- one real typed gateway reconciliation error seam exists inside the
  runtime/container core
- at least one remaining route translation path no longer relies on a generic
  `task_invocation` string bucket
- tests assert on gateway error category rather than only rendered string output

## Validation

- targeted gateway reconciliation error tests
- `./target/debug/effigy docs check-paths docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md docs/specs/batch-cards/350-implement-typed-gateway-reconciliation-and-route-translation-errors.md docs/specs/batch-cards/351-decide-post-gateway-reconciliation-error-boundary.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/016-container-and-runtime-error-taxonomy-and-diagnostics.md`

## Next Task

Closed. Execute `351`.
