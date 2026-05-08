# 537 - Extract Workspace Host Integration Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Start splitting `crates/effigy-containers/src/workspace.rs` by moving
host-integration mount helpers into a focused workspace module without changing
generated compose behavior.

## Scope

- create workspace submodules under `crates/effigy-containers/src/workspace/`
  or the smallest compatible module shape
- move host integration helpers where dependencies remain clean:
  - SSH agent mount rendering
  - SSH config and known_hosts mount rendering
  - git config mount rendering
  - composer home mount rendering
  - mkcert CA mount rendering
  - runtime environment injection helpers if they are tightly coupled
- keep `materialize_runtime_workspace_mount_rewrite` public to current callers
- preserve generated PHP workspace host-integration output

## Non-Goals

- no library mount split
- no compose rewrite split
- no isolation/adoption split
- no policy loading changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when host-integration helpers live outside the monolithic
workspace file, host-integration tests pass, and public callers still compile.

## Closeout

Workspace host-integration helpers now live under
`crates/effigy-containers/src/workspace/host_integration.rs`. The main
`workspace.rs` file dropped from 1541 to 973 lines.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-host-check cargo check -p effigy-containers`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-host-libcheck cargo check -p effigy --lib`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-host-test cargo test -p effigy-containers host_git_mount_tests -- --test-threads=1`
- `git diff --check`

## Next Task

Start card
[`538-extract-workspace-library-mounts-module.md`](./538-extract-workspace-library-mounts-module.md).
