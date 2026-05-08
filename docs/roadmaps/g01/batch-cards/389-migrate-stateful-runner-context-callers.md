# 389 - Migrate Stateful Runner Context Callers

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Move the remaining stateful runner cwd/root callers to explicit context-backed
helpers without changing command behavior.

## Scope

- migrate:
  - `exec_command/mod.rs`
  - `container_command/mod.rs`
  - `system_command/workspace/mod.rs`
  - `defer_command.rs`
  - `tasks_command/prepare.rs`
  - `release_command/mod.rs`
- review, but do not force, `execute/preflight/context/discovery.rs` and
  `system_command/workspace_provisioning.rs` because they have extra behavior
  around preflight and workspace install paths
- keep tests and public CLI behavior stable

## Exit Condition

This card is complete when the listed modules no longer use local cwd/root
pairs and focused runner checks pass.

## Closeout

Migrated the listed production callers to `resolve_active_command_context()`,
`resolve_active_repo_root()`, or `active_invocation_cwd()`:

- `exec_command/mod.rs`
- `container_command/mod.rs`
- `system_command/workspace/mod.rs`
- `defer_command.rs`
- `tasks_command/prepare.rs`
- `release_command/mod.rs`

The only scoped `current_working_dir()` match left is test setup in
`defer_command.rs`.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy container_command -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy exec_command -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy defer_command -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy --test cli_output_tests cli_explicitly_deferred_release_bypasses_builtin_release_command -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy workspace_session_cleanup_matrix -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy effective_workspace_repo_override -- --nocapture`

Note: a broad `workspace` filter hit
`workspace_artifact_source_download_bypasses_discoverable_local_repo`, which
failed on missing cached artifact state. Narrow workspace context checks passed.

## Next Task

Implement card `390`: decide the preflight/workspace provisioning boundary.
