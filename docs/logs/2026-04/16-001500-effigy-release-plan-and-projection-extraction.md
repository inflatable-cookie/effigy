# Effigy Release Plan And Projection Extraction

Date: 2026-04-16
Owner: Platform

## Summary

`123` is complete.

Effigy widened `effigy-release` around the heavier release plan and projection
cluster. The prepare/simulate/prepared and execute-plan/executed models no
longer sit entirely inside `release_command.rs`, and their JSON projections now
live in the release crate.

## What Changed

- moved heavier release-facing models into
  [`crates/effigy-release`](../../../crates/effigy-release/Cargo.toml):
  - `FileMutationPlan`
  - `FileMutationApply`
  - `ReleasePreparePlan`
  - `ReleasePrepared`
  - `ReleaseSimulation`
  - `ReleaseExecutePlan`
  - `ReleaseExecuted`
- moved JSON projection ownership for those heavier surfaces into
  `effigy-release`
- reconnected [`src/runner/release_command.rs`](../../../src/runner/release_command.rs)
  so the CLI still serves the same release JSON contract while the crate owns
  the shape

## Why The Next Batch Is Persistence And Orchestration

This batch removes most of the release-facing data and projection ownership
from `runner`, but one heavier seam still remains.

What still sits heavily in `release_command.rs`:

- prepared-state persistence
- prepared-state fingerprint capture and drift comparison
- deeper release execution orchestration

That persistence/orchestration cluster is now the next honest seam.

## Current State

- active strict lane: `g02.010`
- active ready card: `124`
- queued release card: `115`

## Validation

- `cargo test -p effigy-release`
- `cargo test release_command --lib`
- `cargo test --test cli_output_tests release`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `heavier release plan models and JSON projections centered in release_command.rs`
  to `heavier release plan models and JSON projections centered in effigy-release with runner adapters`
- remains open:
  - release state persistence and execution orchestration extraction
  - final modularization boundary decision
  - eventual resume of `g02.007` release closure for `v0.3`

## Next Task

Execute
[`124-implement-effigy-release-state-persistence-and-orchestration-extraction.md`](../../specs/batch-cards/124-implement-effigy-release-state-persistence-and-orchestration-extraction.md)
to widen `effigy-release` around release state persistence and execution
orchestration.
