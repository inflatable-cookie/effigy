# 540 - Extract Workspace Compose Rewrite Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-07

## Goal

Continue splitting `crates/effigy-containers/src/workspace.rs` by moving
compose volume/environment rewrite helpers into a focused workspace module
without changing generated compose output.

## Scope

- create `crates/effigy-containers/src/workspace/compose_rewrite.rs`
- move compose rewrite helpers where dependencies remain clean:
  - `rewrite_workspace_service_volumes`
  - `inject_workspace_service_environment`
  - `inject_workspace_named_volumes`
  - `compact_workspace_named_volume_mounts`
  - compose-relative path normalization helpers
  - bind-mount source resolution helpers
- keep `materialize_runtime_workspace_mount_rewrite` behavior stable
- preserve generated compose output and named-volume compaction behavior

## Non-Goals

- no host-integration changes
- no isolation changes
- no policy loading changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when compose rewrite helpers live outside the monolithic
workspace file, generated compose tests pass, and public callers still compile.

## Validation

- `cargo check -p effigy-containers`
- `cargo check -p effigy --lib`
- `cargo test -p effigy-containers generated_compose_underlay_shape_keeps_runtime_paths_and_external_mounts_stable -- --test-threads=1`
- `cargo test -p effigy-containers direct_compose_policy_rewrites_workspace_mounts_from_manifest_contract -- --test-threads=1`
- `git diff --check`

## Next Task

Extract the workspace compose rewrite module.
