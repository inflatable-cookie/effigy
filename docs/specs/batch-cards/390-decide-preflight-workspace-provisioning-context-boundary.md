# 390 - Decide Preflight Workspace Provisioning Context Boundary

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Decide whether the remaining preflight and workspace provisioning cwd/root
callers should migrate now or stay as explicitly-owned lower-level seams.

## Scope

- inspect `execute/preflight/context/discovery.rs`
- inspect `system_command/workspace_provisioning.rs`
- inspect any remaining production `current_working_dir()` or
  `resolve_repo_root()` callers under `src/runner`
- choose the next implementation card:
  - migrate preflight context discovery
  - migrate workspace provisioning path handling
  - or pivot to execution-surface policy cleanup
- no behavior changes

## Exit Condition

This card is complete when the remaining boundary is explicit and the next
ready card has a narrow write set.

## Decision

Migrate preflight now. Keep workspace provisioning explicit.

Reasoning:

- preflight discovery resolves task execution target state, so it should use the
  shared command-context helper
- workspace provisioning uses cwd as an operator-local Effigy checkout discovery
  hint, not as task target truth
- keeping that probe local is acceptable while it remains inside
  `workspace_provisioning`

## Closeout

Added `resolve_command_context_from_cwd()` and moved
`execute/preflight/context/discovery.rs` onto it.

Remaining production cwd/root probes:

- `command_context` helpers
- `builtin_ports` port implementation for builtins
- `workspace_provisioning` local Effigy checkout discovery

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy execute:: -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy defer_command -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy task_run_json_contract_reclaims_expired_workspace_lock_lease -- --nocapture`

## Next Task

Implement card `391`: decide the execution-surface policy bridge cleanup.
