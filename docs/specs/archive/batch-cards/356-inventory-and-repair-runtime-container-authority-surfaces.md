# 356 Inventory And Repair Runtime/Container Authority Surfaces

Status: complete
Updated: 2026-05-02
Roadmap: `g03.017`
Spec: `docs/specs/031-architecture-map-and-authority-surface-repair-strict-lane.md`

## Objective

Inventory the stale architecture authority surfaces, then repair the front
 doors and the highest-signal runtime/container ownership map in one bounded
 batch.

## In Scope

- audit the current architecture front doors against the live post-hardening
  code seams
- repair or replace the highest-signal stale authority surface, starting with
  the runtime/container package or module map
- update architecture front doors so they point at the repaired authority docs
- make the active ownership surfaces explicit for:
  - runner orchestration
  - runtime/session context
  - typed container assembly
  - workspace session/provisioning split
  - runtime/container error families

## Out Of Scope

- new runtime/container behavior
- broad guides cleanup
- final hardening proof-matrix work
- low-signal repo-wide wording churn

## Acceptance Criteria

- at least one currently stale architecture authority surface is replaced or
  repaired into something trustworthy
- the architecture front doors no longer point readers mainly at stale package
  truth
- the next boundary after the first repair batch is explicit

## Validation

- `./target/debug/effigy docs check-paths docs/specs/031-architecture-map-and-authority-surface-repair-strict-lane.md docs/specs/batch-cards/356-inventory-and-repair-runtime-container-authority-surfaces.md docs/specs/batch-cards/357-decide-post-architecture-authority-foundation-boundary.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/017-architecture-map-and-authority-surface-repair.md`

## Next Task

Closed. Execute `357`.
