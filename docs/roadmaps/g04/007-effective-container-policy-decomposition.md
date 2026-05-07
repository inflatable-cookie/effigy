# 007 - Effective Container Policy Decomposition

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-07
Depends on: [`006-rhai-host-api-split-and-callback-purity.md`](./006-rhai-host-api-split-and-callback-purity.md)

## Goal

Split `effigy-containers` into clear policy, workspace, runtime, and backend
ownership modules.

## Scope

- move policy model/load/validation/project/inline workspace into separate
  modules
- move workspace host integration and compose rewrite into separate modules
- move DNS/eject/runtime helpers out of `lib.rs`
- keep public exports stable during migration
- add comments only where module ownership is not obvious

## Migration Targets

- `crates/effigy-containers/src/lib.rs`
- `crates/effigy-containers/src/workspace.rs`
- `crates/effigy-containers/src/policy_support.rs`
- `crates/effigy-containers/src/exec.rs`

## Acceptance Criteria

- `lib.rs` is mostly exports and top-level orchestration under 500 lines
- `workspace.rs` is split under domain modules
- public tests still pass
- package map is updated

## Validation

- `cargo test -p effigy-containers`
- focused compose/workspace/policy tests

## Next Task

Start roadmap
[`008-manager-backed-runtime-read-write-shell.md`](./008-manager-backed-runtime-read-write-shell.md).
