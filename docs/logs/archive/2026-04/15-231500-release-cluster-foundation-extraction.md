# Release Cluster Foundation Extraction

Date: 2026-04-15
Owner: Platform

## Summary

`120` is complete.

Effigy now has real `effigy-containers` and `effigy-distribution` workspace
crates in live use. The release-blocking cluster no longer sits entirely
inside `runner`.

## What Changed

- added [`crates/effigy-containers`](../../../../crates/effigy-containers/Cargo.toml)
- added [`crates/effigy-distribution`](../../../../crates/effigy-distribution/Cargo.toml)
- moved container policy ownership into `effigy-containers`:
  - effective container policy model
  - policy loading from manifest
  - mount and compose-file validation
  - attach-mode resolution
- moved distribution policy ownership into `effigy-distribution`:
  - effective distribution policy model
  - policy loading from manifest
  - manifest/default normalization
  - override helpers
  - artifact-pattern derivation
- reconnected [`src/runner/container_command.rs`](../../../../src/runner/container_command.rs)
  and
  [`src/runner/distribution_command.rs`](../../../../src/runner/distribution_command.rs)
  as adapter layers over those crates
- hardened one exposed startup edge while reconnecting the container adapter:
  startup-phase SIGINT is now trapped before policy loading finishes, so the
  attached container path still exits cleanly instead of dying with signal `2`

## Why The Next Batch Is Effigy Release

This batch removed the policy layer from two parts of the release-blocking
cluster, but it also made the remaining pressure more obvious.

`release_command.rs` is now the largest release-blocking domain still fully
centered in `runner`.

That makes the next honest move narrower:

- stop talking about the release cluster as one blob
- extract the first real `effigy-release` foundation next

## Current State

- active strict lane: `g02.010`
- active ready card: `121`
- queued release card: `115`

## Validation

- `cargo test -p effigy-containers`
- `cargo test -p effigy-distribution`
- `cargo test --test cli_output_tests container`
- `cargo test --test cli_output_tests distribution`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `release-blocking cluster policy ownership still centered in runner`
  to `container and distribution policy ownership centered in dedicated domain crates with runner adapters`
- remains open:
  - first `effigy-release` extraction
  - later demo extraction
  - modularization pause decision
  - eventual resume of `g02.007` release closure for `v0.3`

## Next Task

Execute
[`121-implement-effigy-release-foundation-extraction.md`](../../../specs/batch-cards/121-implement-effigy-release-foundation-extraction.md)
to start the first dedicated `effigy-release` extraction.
