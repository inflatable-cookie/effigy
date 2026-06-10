# Effigy Release Foundation Extraction

Date: 2026-04-15
Owner: Platform

## Summary

`121` is complete.

Effigy now has a real `effigy-release` crate in live use. Release config
resolution and release gate execution no longer sit entirely inside
`release_command.rs`.

## What Changed

- added [`crates/effigy-release`](../../../../crates/effigy-release/Cargo.toml)
- moved release config ownership into `effigy-release`:
  - version-file detection
  - version-path defaults and validation
  - release changelog path resolution
  - gate resolution
  - sync-file resolution
  - tag-format validation
- moved release gate execution into `effigy-release`:
  - gate result and report contracts
  - gate command execution
  - gate blocker helpers
- reconnected [`src/runner/release_command.rs`](../../../../src/runner/release_command.rs)
  as an adapter over that backbone while preserving current release progress
  output

## Why The Next Batch Is Release State And Projections

This batch made `effigy-release` real, but it also made the remaining release
mass more obvious.

What still sits heavily in `release_command.rs`:

- release context/state ownership
- prepare/execute/resume plan models
- text/json projection ownership

That is now the clearest next seam.

## Current State

- active strict lane: `g02.010`
- active ready card: `122`
- queued release card: `115`

## Validation

- `cargo test -p effigy-release`
- `cargo test release_command --lib`
- `cargo test --test cli_output_tests release`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `release config and gate execution centered in release_command.rs`
  to `release config and gate execution centered in a dedicated effigy-release crate with runner adapters`
- remains open:
  - release state and projection extraction
  - deeper release orchestration movement
  - modularization pause decision
  - eventual resume of `g02.007` release closure for `v0.3`

## Next Task

Execute
[`122-implement-effigy-release-state-and-projection-extraction.md`](../../../specs/batch-cards/122-implement-effigy-release-state-and-projection-extraction.md)
to widen `effigy-release` around release state and projection ownership.
