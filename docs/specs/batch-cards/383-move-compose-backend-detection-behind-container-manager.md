# 383 - Move Compose Backend Detection Behind Container Manager

Lane: [`038-plugin-ready-container-manager-facade-strict-lane.md`](../038-plugin-ready-container-manager-facade-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

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

## Next Task

Implement card `384`: migrate container lifecycle status/up/down/logs through
`ContainerManager`.
