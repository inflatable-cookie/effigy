# Effigy Release State And Projection Extraction

Date: 2026-04-15
Owner: Platform

## Summary

`122` is complete.

Effigy widened `effigy-release` beyond config and gates. The simpler release
result models and their JSON projections no longer sit entirely inside
`release_command.rs`.

## What Changed

- moved simpler release-facing result models into
  [`crates/effigy-release`](../../../../crates/effigy-release/Cargo.toml):
  - `ReleaseStatus`
  - `ReleaseGateRun`
  - `ReleaseVerifyInstall`
  - `VerificationStepResult`
- moved JSON projection ownership for those surfaces into `effigy-release`
- reconnected [`src/runner/release_command.rs`](../../../../src/runner/release_command.rs)
  so the CLI still uses the same JSON contract while the crate owns the shape

## Why The Next Batch Is The Heavier Plan Cluster

This batch removed the smaller result/projection surfaces cleanly, but the
remaining release mass is still obvious.

What still sits heavily in `release_command.rs`:

- prepare plan models
- simulation and prepared-result projections
- execute-plan and resume projections
- executed-result projection

That heavier plan/projection cluster is the next honest seam.

## Current State

- active strict lane: `g02.010`
- active ready card: `123`
- queued release card: `115`

## Validation

- `cargo test -p effigy-release`
- `cargo test release_command --lib`
- `cargo test --test cli_output_tests release`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `simpler release result models and JSON projections centered in release_command.rs`
  to `simpler release result models and JSON projections centered in effigy-release with runner adapters`
- remains open:
  - heavier release plan/projection extraction
  - deeper release orchestration movement
  - modularization pause decision
  - eventual resume of `g02.007` release closure for `v0.3`

## Next Task

Execute
[`123-implement-effigy-release-plan-and-projection-extraction.md`](../../../specs/batch-cards/123-implement-effigy-release-plan-and-projection-extraction.md)
to widen `effigy-release` around the heavier release plan and projection
cluster.
