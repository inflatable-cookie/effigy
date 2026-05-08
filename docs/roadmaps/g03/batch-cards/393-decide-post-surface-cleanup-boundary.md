# 393 - Decide Post Surface Cleanup Boundary

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Choose the next concrete cleanup target in `g03.033` after removing the unused
runtime-prep surface bridge.

## Scope

- inspect the remaining `g03.033` cleanup targets
- compare remaining runner/container hotspots against already-migrated context,
  manager, and task-request surfaces
- choose one narrow implementation card
- avoid implementation changes in this decision card

## Exit Condition

This card is complete when the next cleanup target has a bounded write set and
the active lane points at its implementation card.

## Decision

Move the runtime inspection invocation branching in
`crates/effigy-containers/src/exec.rs` behind `ContainerManager`.

Reasoning:

- `exec.rs` remains the largest container hotspot and still branches on
  `resolve_compose_backend()` for Docker vs Colima runtime inspection commands
- the manager already has `runtime_process_invocation(...)`, which is the right
  boundary for `docker ps`, `docker inspect`, and `docker stats` shape
- this is narrower than splitting all of `exec.rs`
- it reduces backend branching without changing parsing, status rendering, or
  lifecycle behavior

## Deferred

- full `exec.rs` decomposition
- workspace provisioning split
- standard/managed pipeline decomposition
- public operation-report JSON

## Next Task

Implement card `394`: move container inspection invocation branching behind
`ContainerManager`.
