# 107 Implement Colima Container Foundation

Status: archived
Updated: 2026-04-15
Roadmap: `g02.006`
Spec: `docs/specs/archive/006-colima-container-environment-strict-lane.md`

## Objective

Implement the first bounded foundation for the v1 `effigy container` surface.

## In Scope

- add the `container` command surface for named and default containers
- add manifest parsing for the v1 `[containers]` registry
- implement Colima profile startup/use plus compose-based environment bring-up
- implement explicit host-facing port/mount and `primary_service` handling
- implement the attached owner-session lifecycle strongly enough for one real
  consumer proof

## Out Of Scope

- broad multi-driver abstraction
- detached/background daemon orchestration
- per-service health DSL beyond the settled v1 environment gate
- broad rollout across multiple consumer repos

## Acceptance Criteria

- one named container environment can be brought up and down through
  `effigy container ...`
- omitted container names resolve through a manifest default when present
- the attached owner-session lifecycle is real enough to prove shutdown on
  owner exit
- one real consumer repo can exercise the foundation honestly

## Validation

- targeted unit/integration tests for the new command and manifest surface
- one real consumer proof
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute the next `g02.006` ready card to widen the attached-session UX/TUI
surface and repo-owned task composition on top of the shipped foundation.
