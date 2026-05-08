# 349 Decide Post-Workspace Handoff And Lease Error Boundary

Status: archived
Updated: 2026-05-02
Roadmap: `g03.016`
Spec: `docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md`

## Objective

Decide whether `g03.016` needs another bounded slice after the workspace
handoff and lease error batch lands.

## In Scope

- inspect the landed `348` surface against the roadmap and strict-lane target
- decide whether another bounded error-taxonomy slice is still needed
- refresh the active strict-lane and front-door surfaces to match that
  decision

## Out Of Scope

- architecture-map repair
- crate extraction
- new runtime/container features

## Acceptance Criteria

- the next honest boundary after `348` is explicit
- no stale ready card is left behind
- the strict-lane state matches reality

## Validation

- `./target/debug/effigy docs check-paths docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md docs/roadmaps/g03/batch-cards/348-implement-typed-workspace-handoff-and-lease-error-translation.md docs/roadmaps/g03/batch-cards/349-decide-post-workspace-handoff-and-lease-error-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/016-container-and-runtime-error-taxonomy-and-diagnostics.md`

## Next Task

Closed. Execute `350`.
