# 357 Decide Post-Architecture Authority Foundation Boundary

Status: archived
Updated: 2026-05-02
Roadmap: `g03.017`
Spec: `docs/specs/031-architecture-map-and-authority-surface-repair-strict-lane.md`

## Objective

Decide whether the first repaired authority batch is enough to close
 `g03.017`, or whether one more bounded architecture-authority slice is still
 needed before handing off to `g03.018`.

## In Scope

- inspect the landed `356` repair surface against the roadmap and strict-lane
  target
- decide whether another bounded architecture-authority slice is still needed
- refresh the active strict-lane and front-door surfaces to match that
  decision

## Out Of Scope

- new runtime/container behavior
- broad documentation cleanup outside architecture authority
- proof-matrix execution

## Acceptance Criteria

- the next honest boundary after `356` is explicit
- no stale ready card is left behind
- the strict-lane state matches reality

## Validation

- `./target/debug/effigy docs check-paths docs/specs/031-architecture-map-and-authority-surface-repair-strict-lane.md docs/roadmaps/g03/batch-cards/356-inventory-and-repair-runtime-container-authority-surfaces.md docs/roadmaps/g03/batch-cards/357-decide-post-architecture-authority-foundation-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/017-architecture-map-and-authority-surface-repair.md`

## Next Task

Closed. Promote `g03.018`.
