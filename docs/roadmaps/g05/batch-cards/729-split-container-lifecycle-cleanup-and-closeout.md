# 729 - Split Container Lifecycle Cleanup And Closeout

Roadmap: [`../011-container-lifecycle-owner-split.md`](../011-container-lifecycle-owner-split.md)
Strict lane: [`../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md`](../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md)
Contract: [`../../../contracts/023-container-command-decomposition-contract.md`](../../../contracts/023-container-command-decomposition-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Finish the lifecycle split by extracting cleanup and closeout helpers and
revalidating the owner layout.

## Completed

- Added `container_command/closeout.rs` for lifecycle cleanup and interrupted-up
  closeout ownership.
- Moved startup-cleanup error shaping, reset confirmation, and interrupted-up
  text rendering out of `lifecycle.rs`.
- Kept lifecycle dispatch stable while reducing the remaining mixed owner surface.

## Next Task

Execute `730`.
