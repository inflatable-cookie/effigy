# 296 Implement Managed Dev Readiness UX Foundation

Status: landed
Updated: 2026-04-18
Roadmap: `g02.013`
Spec: `docs/specs/013-dev-front-door-and-managed-lifecycle-strict-lane.md`

## Objective

Land the next bounded `g02.013` slice by making a managed dev task able to
wait for container readiness and surface one honest ready message inside the
managed runtime.

## In Scope

- add bounded managed-task readiness metadata under `tasks.<name>.managed`
- let a managed dev task wait on the task-owned workspace container health path
  before declaring ready
- project a ready-state message through the managed runtime on the product path
- cover the readiness contract in schema/help/tests where needed

## Out Of Scope

- gateway auto-start
- broader multi-service health orchestration beyond the task-owned container
  surface
- real-project proof work

## Acceptance Criteria

- manifests can declare bounded readiness metadata on a managed dev task
- the managed runtime can wait for the target workspace container to become ready
  before presenting the task as ready
- a manifest-owned ready message appears on the product path without pretending
  gateway automation is already shipped
- docs/tests/output surfaces describe the bounded readiness behavior clearly

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Result

Managed dev tasks can now declare bounded readiness metadata through
`managed.health_wait` plus `managed.ready_message`, and that contract is wired
through plan/schema/docs plus the managed runtime product path.

## Next Task

Execute `297` to decide whether gateway auto-start or real-project proof is
the next bounded `g02.013` move.
