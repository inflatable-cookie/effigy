# 346 Implement Typed Container Surface And Policy Translation Errors

Status: complete
Updated: 2026-05-02
Roadmap: `g03.016`
Spec: `docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md`

## Objective

Land the next bounded typed error slice for container surface resolution and
policy translation.

## In Scope

- introduce the next explicit error-family split for high-signal container
  surface and policy failures
- center the batch on:
  - `effigy exec` container-surface resolution failures
  - missing or ambiguous dev-container selection
  - missing named container selection
  - container-not-running operator errors
  - generated/policy validation translation where runner code currently
    flattens container-policy failures into generic invocation strings
- add focused category-level tests for the newly typed seams

## Out Of Scope

- full `effigy-containers` error taxonomy cleanup
- broad wording polish across unrelated commands
- architecture authority repair

## Acceptance Criteria

- one real typed container-surface error seam exists beyond runtime prep
- at least one high-signal policy-translation path no longer relies on a
  generic `task_invocation` string bucket
- tests assert on container-surface failure category rather than only rendered
  string output

## Validation

- targeted exec/container-surface error tests
- targeted container-policy translation tests
- `./target/debug/effigy docs check-paths docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md docs/specs/batch-cards/346-implement-typed-container-surface-and-policy-translation-errors.md docs/specs/batch-cards/347-decide-post-typed-container-surface-and-policy-boundary.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/016-container-and-runtime-error-taxonomy-and-diagnostics.md`

## Next Task

Closed. Execute `347`.
