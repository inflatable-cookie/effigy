# 068 - Container Command Decomposition Strict Lane

Roadmap: [`g04.025`](../roadmaps/g04/025-container-command-decomposition.md)

Status: Active
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

## Current Ready Card

- `646` extract the cache command family into `cache.rs`

## Execution Chain

- `644` complete: opened the lane, promoted the contract anchor, and selected
  the first real extraction slice
- `645` complete: locked the structural-only boundary, target module ownership,
  extraction order, and thin-dispatcher rule for `mod.rs`
- `646` ready: extract `cache list` and `cache prune` into `cache.rs` with
  focused container-command proof coverage

## Exit Condition

This lane is complete when cache, volume, lifecycle, and shared scope helpers
are extracted behind stable module owners, `mod.rs` is a thin dispatcher, and
focused container tests show no user-facing drift.

## Next Task

Execute `646` to extract the cache command family into `cache.rs`.
