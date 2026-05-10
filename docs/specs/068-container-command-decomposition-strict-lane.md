# 068 - Container Command Decomposition Strict Lane

Roadmap: [`g04.025`](../roadmaps/g04/025-container-command-decomposition.md)

Status: Complete
Owner: Platform
Created: 2026-05-10

## Purpose

Split `src/runner/container_command/` into operator-domain modules without
changing the shipped container behavior.

## Hard Boundaries

- no CLI grammar changes
- no JSON schema changes
- no behavior changes beyond tiny equivalent refactor fallout
- no `.github/workflows/` edits
- no release execution

## Execution Chain

- `644` complete: opened the lane, promoted the contract anchor, and selected
  the first real extraction slice
- `645` complete: locked the structural-only boundary, target module ownership,
  extraction order, and thin-dispatcher rule for `mod.rs`
- `646` complete: extracted `cache list` and `cache prune` into `cache.rs`
  with focused container-command proof coverage
- `647` complete: extracted `volume list` and `volume prune` into `volume.rs`
  with focused container-command proof coverage
- `648` complete: moved the lifecycle command family out of `mod.rs` and kept
  the shipped fallback behavior unchanged
- `649` complete: extracted the shared repo-root versus cwd fallback helper for
  status, down, and cache inventory
- `650` complete: moved the remaining data-family dispatch into `data.rs` and
  closed the thin-dispatcher target for `mod.rs`

## Exit Condition

This lane is complete when cache, volume, lifecycle, and shared scope helpers
are extracted behind stable module owners, `mod.rs` is a thin dispatcher, and
focused container tests show no user-facing drift.
