# 648 - Extract Container Lifecycle Command Family

Roadmap: [`../025-container-command-decomposition.md`](../025-container-command-decomposition.md)
Strict lane: [`../../../specs/068-container-command-decomposition-strict-lane.md`](../../../specs/068-container-command-decomposition-strict-lane.md)
Contract: [`../../../contracts/023-container-command-decomposition-contract.md`](../../../contracts/023-container-command-decomposition-contract.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Purpose

Move the lifecycle family out of `container_command/mod.rs` so `mod.rs` keeps
shrinking toward a thin dispatcher.

## Scope

- extract `up`, `down`, `status`, `stats`, `logs`, `shell`, `reset`, and `eject`
  entrypoints into a lifecycle owner
- keep the current repo-root versus cwd fallback behavior unchanged
- leave shared fallback-helper extraction and final `mod.rs` cleanup for later cards

## Acceptance

- lifecycle entrypoints live outside `mod.rs`
- `mod.rs` shrinks again and keeps only dispatch glue plus any tiny shared seams
- focused lifecycle proofs stay green
