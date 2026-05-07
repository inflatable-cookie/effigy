# 396 - Move Colima Start Runtime Selection Behind Manager

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Remove the legacy compose-backend lookup from Colima start command assembly.

## Scope

- add a small `ContainerManager` API that resolves the Colima VM runtime name
  from backend detection
- update `crates/effigy-containers/src/colima.rs` to use that API instead of
  `resolve_compose_backend()`
- preserve current output:
  - Docker backend -> `--runtime docker`
  - Colima/nerdctl backend -> `--runtime containerd`
- keep Colima profile provisioning and resource planning unchanged

## Exit Condition

This card is complete when `colima.rs` no longer imports
`resolve_compose_backend()` or `ComposeBackend`, and focused container-manager
and container tests pass.

## Closeout

Colima start command assembly now asks `ContainerManager` for the VM runtime
name instead of calling the legacy compose backend resolver.

Current behavior is unchanged:

- Docker backend emits `--runtime docker`
- Colima/nerdctl backend emits `--runtime containerd`

The Colima tests also now serialize mutations of
`EFFIGY_INTERNAL_HOST_MEMORY_BYTES`, which removes a parallel test race exposed
by the focused validation run.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-container-manager -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-containers colima -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `rg "resolve_compose_backend\(\)|ComposeBackend" crates/effigy-containers/src/colima.rs -n`

## Next Task

Decide whether `g03.033` should close now or take one more cleanup slice.
