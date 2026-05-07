# 004 - Container Operation Pipeline

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-07
Depends on: [`003-runtime-activation-pipeline.md`](./003-runtime-activation-pipeline.md)

## Goal

Make container commands thin wrappers over typed operation requests.

## Scope

- add `crates/effigy-container-ops`
- define operation request, plan, kind, safety policy, side-effect class, and
  report types
- move lifecycle/status/logs/shell/exec/data/cache plan construction out of
  runner command modules
- route backend work through `ContainerManager`
- preserve current CLI and JSON behavior unless a card explicitly documents a
  cleanup break

## Migration Targets

- `src/runner/container_command/mod.rs`
- `src/runner/container_command/lifecycle.rs`
- `src/runner/container_command/support.rs`
- `src/runner/container_command/data.rs`
- `crates/effigy-runtime/src/read.rs`
- `crates/effigy-runtime/src/write.rs`
- `crates/effigy-runtime/src/shell.rs`
- `crates/effigy-runtime/src/data.rs`

## Acceptance Criteria

- `container_command/mod.rs` is dispatch glue only
- runner code does not construct Docker, Colima, or nerdctl commands
- runner code does not call `compose_args` for container operations
- operation plans expose side-effect class and confirmation policy

## Validation

- `cargo test -p effigy-container-ops`
- `cargo test -p effigy-container-manager`
- `cargo test -p effigy --lib container_command`

## Next Task

Closed by card
[`501-remove-final-runner-compose-runtime-helper-drift.md`](../../specs/batch-cards/501-remove-final-runner-compose-runtime-helper-drift.md).
