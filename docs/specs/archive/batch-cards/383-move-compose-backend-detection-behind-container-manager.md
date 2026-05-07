# 383 - Move Compose Backend Detection Behind Container Manager

Lane: [`038-plugin-ready-container-manager-facade-strict-lane.md`](../038-plugin-ready-container-manager-facade-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Move existing compose backend detection and invocation shape behind
`ContainerManager` without changing public container command behavior.

## Scope

- inventory current `resolve_compose_backend()` callers and Docker/Colima
  process construction seams
- add manager-side detection inputs for explicit override and current default
  behavior
- route existing compose invocation construction through the manager facade
- keep existing `effigy-containers` wrappers temporarily where needed
- add focused tests for Docker and Colima backend selection parity
- do not migrate every container operation yet

## Exit Condition

This card is complete when existing compose backend selection can be expressed
through `ContainerManager`, tests prove current Docker/Colima selection parity,
and the next card can migrate lifecycle commands through the facade.

## Closeout

Compose backend selection now uses `ContainerBackendDetection` and
`ContainerBackendRegistry` from `effigy-container-manager`.

`effigy-containers::compose` remains as a compatibility wrapper for existing
callers, but backend override parsing, Docker fallback selection, host CLI
program resolution, and Docker-vs-Colima process wrapping now sit in the
manager crate.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-container-manager-target cargo test -p effigy-container-manager -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-containers-target cargo test -p effigy-containers compose -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next Task

Implement card `384`: migrate container lifecycle status/up/down/logs through
`ContainerManager`.
