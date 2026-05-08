# 348 Implement Typed Workspace Handoff And Lease Error Translation

Status: archived
Updated: 2026-05-02
Roadmap: `g03.016`
Spec: `docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md`

## Objective

Land the next bounded typed error slice for workspace handoff and host-container
lease failures.

## In Scope

- introduce the next explicit error-family split for high-signal workspace
  session and lease failures
- center the batch on:
  - public workspace handoff cleanup translation
  - shell-plus-cleanup combined failure reporting
  - host-container lease encode/write failure translation
  - one public workspace handoff or cleanup seam that still collapses into a
    generic invocation string
- add focused category-level tests for the newly typed seams

## Out Of Scope

- full gateway-registration error taxonomy
- broad wording polish across unrelated commands
- architecture authority repair

## Acceptance Criteria

- one real typed workspace-handoff or lease error seam exists beyond runtime
  prep and exec-surface selection
- at least one remaining session/lease translation path no longer relies on a
  generic `task_invocation` string bucket
- tests assert on handoff or lease failure category rather than only rendered
  string output

## Validation

- targeted workspace-session error tests
- targeted host-container lease error tests
- `./target/debug/effigy docs check-paths docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md docs/roadmaps/g03/batch-cards/348-implement-typed-workspace-handoff-and-lease-error-translation.md docs/roadmaps/g03/batch-cards/349-decide-post-workspace-handoff-and-lease-error-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/016-container-and-runtime-error-taxonomy-and-diagnostics.md`

## Next Task

Closed. Execute `349`.
