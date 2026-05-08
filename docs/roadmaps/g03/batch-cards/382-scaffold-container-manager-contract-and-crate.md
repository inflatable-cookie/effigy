# 382 - Scaffold Container Manager Contract And Crate

Lane: [`038-plugin-ready-container-manager-facade-strict-lane.md`](../038-plugin-ready-container-manager-facade-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Open the `g03.031` lane and create the first dependency-light
`effigy-container-manager` facade slice.

## Scope

- add `docs/contracts/012-container-manager-contract.md`
- add the `038` strict lane
- add `crates/effigy-container-manager`
- define the manager, backend trait, registry, backend id, request, runtime
  state, interrupt policy, and operation report types
- add static Docker Compose and Colima/nerdctl backend stubs
- prove backend override selection, invocation shape, and required report
  fields
- do not wire runner container commands yet

## Exit Condition

This card is complete when the contract and crate exist, the crate builds
independently, and the next card can migrate existing backend detection through
the facade.

## Closeout

`g03.031` is open as lane `038`. `docs/contracts/012-container-manager-contract.md`
now defines the manager facade boundary.

`crates/effigy-container-manager` now defines:

- `ContainerManager`
- `ContainerBackend`
- `ContainerBackendRegistry`
- `BackendId`
- typed manager and operation request shells
- `ContainerRuntimeState`
- `ContainerOperationReport`
- `ContainerInterruptPolicy`
- static Docker Compose and Colima/nerdctl backend stubs

The first tests prove default backend registration, explicit backend override,
stable compose invocation shape, and required report identity fields.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-container-manager-target cargo test -p effigy-container-manager -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `git diff --check`

## Next Task

Implement card `383`: move existing compose backend detection and invocation
shape behind `ContainerManager`.
