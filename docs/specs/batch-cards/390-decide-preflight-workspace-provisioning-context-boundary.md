# 390 - Decide Preflight Workspace Provisioning Context Boundary

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

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

## Next Task

Choose the next implementation card.
