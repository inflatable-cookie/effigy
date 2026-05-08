# 538 - Extract Workspace Library Mounts Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Continue splitting `crates/effigy-containers/src/workspace.rs` by moving
user-global library mount rendering into a focused workspace module without
changing generated compose behavior.

## Scope

- create `crates/effigy-containers/src/workspace/library_mounts.rs`
- move library mount helpers where dependencies remain clean:
  - `WORKSPACE_LIBRARIES_ROOT`
  - `build_library_mounts`
- keep runtime workspace mount rewrite behavior stable
- preserve library mount collision and missing-path behavior

## Non-Goals

- no compose rewrite split
- no isolation/adoption split
- no host-integration changes
- no policy loading changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when library mount helpers live outside the monolithic
workspace file, library mount tests pass, and public callers still compile.

## Closeout

Workspace library mount helpers now live under
`crates/effigy-containers/src/workspace/library_mounts.rs`. The main
`workspace.rs` file dropped from 973 to 910 lines.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-library-check cargo check -p effigy-containers`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-library-libcheck cargo check -p effigy --lib`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-library-test cargo test -p effigy-containers library_mount_tests -- --test-threads=1`
- `git diff --check`

## Next Task

Start card
[`539-extract-workspace-isolation-mounts-module.md`](./539-extract-workspace-isolation-mounts-module.md).
