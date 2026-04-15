# Effigy Release State Persistence And Orchestration Extraction

Date: 2026-04-16
Owner: Platform

## Summary

`124` is complete.

Effigy widened `effigy-release` around release prepared-state persistence,
source fingerprint drift handling, and mutation application. That ownership no
longer sits entirely in `release_command.rs`.

## What Changed

- moved prepared-state persistence contracts into
  [`crates/effigy-release`](../../../crates/effigy-release/Cargo.toml):
  - `ReleasePreparedState`
  - `ReleasePreparedSourceFingerprints`
  - `ReleasePreparedFileFingerprint`
  - prepared-state read/write helpers
- moved release mutation execution helpers into `effigy-release`:
  - mutation snapshots
  - changed-path detection
  - mutation application
- reconnected [`src/runner/release_command.rs`](../../../src/runner/release_command.rs)
  as a thinner adapter:
  - it now supplies git-derived branch and HEAD context
  - it now consumes crate-owned state persistence and drift helpers
  - it no longer owns the prepared-state file format directly
- added focused release-crate coverage for:
  - prepared-state round-trip
  - normalized expected files
  - fingerprint drift detection
  - mutation snapshot behavior

## Why The Next Batch Is A Decision

This batch moved the last obvious release persistence ownership out of
`runner`.

What still sits mainly in `release_command.rs`:

- git-facing release execution steps
- verify-install temp-fixture orchestration
- interactive text review and shell-facing progress/render flow

That remainder may be honest shell/runtime adapter work rather than more domain
crate debt. The next move should decide that explicitly before opening another
extraction card.

## Current State

- active strict lane: `g02.010`
- active ready card: `125`
- queued release card: `115`

## Validation

- `cargo test -p effigy-release`
- `cargo test release_command --lib`
- `cargo test --test cli_output_tests release`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `prepared-state persistence and mutation application centered in release_command.rs`
  to `prepared-state persistence and mutation application centered in effigy-release with runner adapters`
- remains open:
  - post-extraction boundary decision for the remaining release shell
  - final modularization boundary decision
  - eventual resume of `g02.007` release closure for `v0.3`

## Next Task

Execute
[`125-decide-post-release-persistence-extraction-boundary.md`](../../specs/batch-cards/125-decide-post-release-persistence-extraction-boundary.md)
to decide whether the remaining release shell is now thin enough for the
modularization boundary checkpoint.
