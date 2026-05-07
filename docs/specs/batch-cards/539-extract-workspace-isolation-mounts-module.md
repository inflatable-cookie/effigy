# 539 - Extract Workspace Isolation Mounts Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Continue splitting `crates/effigy-containers/src/workspace.rs` by moving
system-isolation mount adoption into a focused workspace module without
changing generated compose behavior.

## Scope

- create `crates/effigy-containers/src/workspace/isolation.rs`
- move isolation helpers where dependencies remain clean:
  - `build_isolation_mounts`
  - `resolve_adopted_isolation_repo`
  - `normalize_isolation_relative_path`
  - `isolation_volume_name`
- keep runtime workspace mount rewrite behavior stable
- preserve isolation mount names and error text

## Non-Goals

- no compose rewrite split
- no workspace extra mount split
- no host-integration changes
- no policy loading changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when isolation mount helpers live outside the monolithic
workspace file, relevant container tests pass, and public callers still
compile.

## Closeout

Workspace isolation mount helpers now live under
`crates/effigy-containers/src/workspace/isolation.rs`. The main `workspace.rs`
file dropped from 910 to 750 lines.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-isolation-check cargo check -p effigy-containers`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-isolation-libcheck cargo check -p effigy --lib`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-isolation-test cargo test -p effigy-containers generated_compose_underlay_shape_keeps_runtime_paths_and_external_mounts_stable -- --test-threads=1`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-isolation-full-test cargo test -p effigy-containers -- --test-threads=1`
- `git diff --check`

## Next Task

Start card
[`540-extract-workspace-compose-rewrite-module.md`](./540-extract-workspace-compose-rewrite-module.md).
