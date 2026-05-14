# 728 - Split Container Lifecycle Secrets And Shell Prep

Roadmap: [`../011-container-lifecycle-owner-split.md`](../011-container-lifecycle-owner-split.md)
Strict lane: [`../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md`](../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md)
Contract: [`../../../contracts/023-container-command-decomposition-contract.md`](../../../contracts/023-container-command-decomposition-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Start the real lifecycle split by pulling secret injection and shell-prep logic
out of the monolithic lifecycle owner.

## Completed

- Added `container_command/secret_env.rs` for container secret env resolution and
  its focused tests.
- Added `container_command/shell_prep.rs` for shell session prep, workspace
  refresh checks, exec env assembly, and working-dir mapping logic.
- Removed those owners from `lifecycle.rs` and kept current behavior stable.

## Next Task

Execute `729`.
