# 389 - Migrate Stateful Runner Context Callers

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

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

## Next Task

Decide whether to migrate preflight/workspace provisioning next or pivot to
execution-surface policy cleanup.
