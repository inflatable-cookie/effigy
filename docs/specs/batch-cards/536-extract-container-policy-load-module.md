# 536 - Extract Container Policy Load Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move container policy loading and effective policy assembly out of
`crates/effigy-containers/src/lib.rs` into `policy/load.rs`, leaving `lib.rs`
as exports plus small shared constants.

## Scope

- create `crates/effigy-containers/src/policy/load.rs`
- move policy loading and assembly helpers where dependencies remain clean:
  - `load_container_policy`
  - `load_container_policy_with_workspace`
  - `load_all_container_policies`
  - `load_container_exec_working_dir`
  - `effective_attach_mode`
  - `build_effective_policy`
  - `resolve_library_mounts`
  - host process parsing helpers
  - default workspace and container-name resolution helpers
  - container exec working-dir resolution
- keep public exports stable through `lib.rs`
- preserve task-routing, generated compose, and error text behavior

## Non-Goals

- no workspace module split
- no `policy_support.rs` split
- no runtime DNS/eject changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when policy loading and effective policy assembly live
outside `lib.rs`, public callers still compile, and container policy tests pass.

## Closeout

Container policy loading and effective policy assembly now live under
`crates/effigy-containers/src/policy/load.rs` and the public crate-root exports
remain stable. `lib.rs` dropped from 621 to 126 lines.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-policy-load-check cargo check -p effigy-containers`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-policy-load-libcheck cargo check -p effigy --lib`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-policy-load-test cargo test -p effigy-containers -- --test-threads=1`
- `git diff --check`

## Next Task

Start card
[`537-extract-workspace-host-integration-module.md`](./537-extract-workspace-host-integration-module.md).
