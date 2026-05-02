# 344 Implement Typed Runtime Container Error Foundation

Status: complete
Updated: 2026-05-02
Roadmap: `g03.016`
Spec: `docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md`

## Objective

Land the first bounded typed error foundation for the runtime/container core.

## In Scope

- introduce the first explicit error-family split for high-signal
  runtime/container failures
- center the first batch on:
  - runtime activation and handoff failures
  - workspace/runtime ownership misuse
  - container policy or generated-compose preparation failures where they
    currently collapse into generic invocation strings
- add focused category-level tests for the newly typed seams

## Out Of Scope

- full repo-wide error taxonomy cleanup
- broad wording polish across unrelated commands
- architecture-map repair

## Acceptance Criteria

- one real typed error seam exists in the runtime/container path
- at least one high-signal failure family no longer relies on a generic
  `task_invocation` string bucket
- tests assert on failure category rather than only rendered string output

## Validation

- targeted runtime/container error tests
- targeted workspace/runtime failure tests
- `./target/debug/effigy docs check-paths docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md docs/specs/batch-cards/344-implement-typed-runtime-container-error-foundation.md docs/specs/batch-cards/345-decide-post-typed-runtime-container-error-foundation-boundary.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/016-container-and-runtime-error-taxonomy-and-diagnostics.md`

## Next Task

Closed. Execute `345`.
