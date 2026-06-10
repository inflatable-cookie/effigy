# Manifest Core Foundation Extraction

Date: 2026-04-15
Owner: Platform

## Summary

`119` is complete.

Effigy now has a real `effigy-manifest` crate in live use. Manifest loading,
composition, task-manifest root ownership, and shared config sections no
longer sit entirely inside `runner`.

## What Changed

- added [`crates/effigy-manifest`](../../../../crates/effigy-manifest/Cargo.toml)
- moved the shared manifest surface there:
  - task manifest root types
  - manifest composition and inspection contracts
  - task runtime and test config contracts
  - shared config-section contracts
- replaced [`src/runner/manifest.rs`](../../../../src/runner/manifest.rs) with a
  thin adapter that:
  - re-exports the extracted manifest surface for current runtime call sites
  - maps `ManifestError` back into `RunnerError`
  - keeps task lock-scope policy local to `runner`
- removed the old duplicate `src/runner/manifest/*.rs` implementations

## Why The Next Batch Is The Release Cluster

With `effigy-core`, `effigy-tasks`, and `effigy-manifest` now real, the
remaining modularization pressure is no longer in shared infrastructure.

The biggest remaining architecture debt that still blocks `v0.3` sits in the
release-blocking cluster:

- `container_command.rs`
- `distribution_command.rs`
- `release_command.rs`

That is the next honest extraction target.

## Current State

- active strict lane: `g02.010`
- active ready card: `120`
- queued release card: `115`

## Validation

- `cargo fmt --all --check`
- `cargo test -p effigy-manifest`
- `cargo test resolver_tests --lib`
- `cargo test validate_manifest_schema_accepts_current_repo_manifest --lib`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `manifest ownership and composition still centered in runner` to
  `manifest ownership centered in a dedicated workspace crate with a runner adapter boundary`
- remains open:
  - release-cluster extraction
  - later demo extraction
  - final modularization boundary decision
  - eventual resume of `g02.007` release closure for `v0.3`

## Next Task

Execute
[`120-implement-release-cluster-foundation-extraction.md`](../../../specs/batch-cards/120-implement-release-cluster-foundation-extraction.md)
to start extracting the release-blocking container, distribution, and release
cluster.
