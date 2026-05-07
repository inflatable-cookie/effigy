# 540 - Extract Workspace Compose Rewrite Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
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

- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-compose-check cargo check -p effigy-containers`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-compose-libcheck cargo check -p effigy --lib`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-compose-test-a cargo test -p effigy-containers generated_compose_underlay_shape_keeps_runtime_paths_and_external_mounts_stable -- --test-threads=1`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-workspace-compose-test-b cargo test -p effigy-containers direct_compose_policy_rewrites_workspace_mounts_from_manifest_contract -- --test-threads=1`
- PASS: `git diff --check`

Note: `cargo check -p effigy --lib` still reports the pre-existing
`runtime_activation_report_for_result` dead-code warning.

## Next Task

Start
[`541-extract-generated-compose-source-module.md`](./541-extract-generated-compose-source-module.md).
