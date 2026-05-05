# 385 - Migrate Exec Copy Data Through Manager

Lane: [`038-plugin-ready-container-manager-facade-strict-lane.md`](../038-plugin-ready-container-manager-facade-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Move remaining exec, copy, and data container operation branching behind
`ContainerManager`.

## Scope

- migrate `src/runner/exec_command/transport.rs`
- migrate `src/runner/container_command/support.rs` shared compose and runtime
  volume helpers
- migrate data import/export/pull paths where they shell into services
- keep public CLI behavior unchanged
- add focused tests for Docker and Colima invocation parity
- add an `rg` drift check for direct runner `resolve_compose_backend()` use

## Exit Condition

This card is complete when runner exec/copy/data paths use manager-owned
backend selection and remaining direct backend branching is contained inside
manager or temporary compatibility wrappers.

## Closeout

Runner exec, copy, data, shared compose, runtime volume, and generated image
removal paths now use `ContainerManager` for backend selection and Docker versus
Colima command wrapping.

No runner-level direct `resolve_compose_backend()` or `ComposeBackend` usage
remains under:

- `src/runner/exec_command`
- `src/runner/container_command`
- `crates/effigy-runtime/src/write.rs`

The remaining direct backend logic is contained in manager-owned APIs and
temporary lower-level compatibility wrappers in `effigy-containers`.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-container-manager-target cargo test -p effigy-container-manager -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy container_command -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy exec_command -- --nocapture`
- `rg "resolve_compose_backend|ComposeBackend" src/runner/exec_command src/runner/container_command crates/effigy-runtime/src/write.rs -n`

## Next Task

Implement card `386`: close `g03.031` with drift guards and contract/readme
alignment.
