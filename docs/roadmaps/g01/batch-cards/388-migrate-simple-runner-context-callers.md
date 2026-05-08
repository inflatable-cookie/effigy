# 388 - Migrate Simple Runner Context Callers

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Move simple runner command modules away from local cwd/root wrapper pairs and
onto an explicit runtime-context helper.

## Scope

- add a small runner helper that returns the active runtime context plus
  resolved command root for a repo override
- migrate simple command entry modules first:
  - `system_command.rs`
  - `service_command.rs`
  - `distribution_command/mod.rs`
  - `deploy_command/mod.rs`
  - `contracts_command.rs`
  - `docs_command/mod.rs`
  - `demo_command/entry.rs`
- keep behavior unchanged
- do not migrate `exec`, `container`, `workspace`, `defer`, `tasks prepare`, or
  preflight discovery in this card

## Exit Condition

This card is complete when the simple command modules no longer call
`current_working_dir()` and `resolve_repo_root()` directly, and focused command
tests or `cargo check -p effigy` pass.

## Closeout

Added `command_context::resolve_active_repo_root()` and migrated the simple
command entry modules listed in scope.

Scoped drift check:

```bash
rg "current_working_dir\\(\\)|resolve_repo_root\\(" \
  src/runner/system_command.rs \
  src/runner/service_command.rs \
  src/runner/distribution_command/mod.rs \
  src/runner/deploy_command/mod.rs \
  src/runner/contracts_command.rs \
  src/runner/docs_command/mod.rs \
  src/runner/demo_command -n
```

The command returns no matches.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next Task

Implement card `389`: migrate the remaining stateful cwd/root callers.
