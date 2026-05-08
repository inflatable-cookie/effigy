# 394 - Move Container Inspection Invocations Behind Manager

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Remove direct Docker/Colima branching from container runtime inspection helpers.

## Scope

- update `crates/effigy-containers/src/exec.rs` inspection helpers to use
  `ContainerManager::runtime_process_invocation(...)`
- cover:
  - running compose container listing
  - host working-dir inference through runtime inspect
  - running container stats capture
- preserve existing parser and output behavior
- keep lifecycle, shutdown, and Colima repair behavior unchanged

## Exit Condition

This card is complete when inspection helpers no longer call
`resolve_compose_backend()` directly for `ps`, `inspect`, or `stats`, and
focused `effigy-containers` tests pass.

## Closeout

`crates/effigy-containers/src/exec.rs` now routes runtime inspection command
shape through `ContainerManager::runtime_process_invocation(...)`.

The parsing and report behavior stayed in `effigy-containers`; only the
Docker-vs-Colima invocation construction moved behind the manager facade.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy-containers`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-containers -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `rg "resolve_compose_backend\(\)|ComposeBackend" crates/effigy-containers/src/exec.rs -n`

## Next Task

Decide the next remaining backend-branching cleanup boundary.
