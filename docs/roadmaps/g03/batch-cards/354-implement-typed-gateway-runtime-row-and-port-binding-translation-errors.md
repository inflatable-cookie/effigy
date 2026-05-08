# 354 Implement Typed Gateway Runtime-Row And Port-Binding Translation Errors

Status: archived
Updated: 2026-05-02
Roadmap: `g03.016`
Spec: `docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md`

## Objective

Land the final narrow gateway taxonomy slice for runtime-row discovery,
service-alias lookup, and raw port-binding translation.

## In Scope

- introduce the next explicit typed error seam for:
  - runtime-row discovery from `list_running_compose_containers_for_profile`
  - service-alias lookup failures
  - raw host/container port-binding translation
- add focused category-level tests for the newly typed gateway closeout seams

## Out Of Scope

- full gateway-command taxonomy cleanup outside container runtime reconciliation
- broad wording polish across unrelated commands
- architecture authority repair

## Acceptance Criteria

- the remaining dominant string-first gateway reconciliation seams inside
  `gateway_registration.rs` no longer rely on generic `task_invocation` strings
- tests assert on the new gateway error categories
- the lane is in a position for an honest final closeout decision

## Validation

- targeted gateway runtime-row and port-binding error tests
- `./target/debug/effigy docs check-paths docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md docs/roadmaps/g03/batch-cards/354-implement-typed-gateway-runtime-row-and-port-binding-translation-errors.md docs/roadmaps/g03/batch-cards/355-decide-post-gateway-final-error-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/016-container-and-runtime-error-taxonomy-and-diagnostics.md`

## Next Task

Closed. Execute `355`.
