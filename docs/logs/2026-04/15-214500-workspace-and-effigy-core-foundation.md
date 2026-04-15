# Workspace And Effigy Core Foundation

Date: 2026-04-15
Owner: Platform

## Summary

`117` is complete.

Effigy is now a Cargo workspace and the first shared backbone crate is real.
This batch moved the shared repo/path resolution contracts out of the main
crate and into `crates/effigy-core`.

## What Changed

- added workspace wiring in [`Cargo.toml`](../../../Cargo.toml)
- added [`crates/effigy-core`](../../../crates/effigy-core)
- moved the first shared backbone contracts into `effigy-core`:
  - `PathPresenceCache`
  - path probe helpers
  - path error text helpers
  - repo cwd / canonicalization helpers
  - `ResolvedTarget`, `ResolveError`, and `ResolutionMode`
- reconnected the root crate so it now consumes those contracts through the
  new backbone

## Why This Slice First

The main codebase still centers too much in `src/lib.rs` and `src/runner/`,
but directly extracting release, distribution, containers, or demos first
would have recreated the same shared-resolution and shared-path coupling in a
workspace shape.

This batch established the first reusable backbone without pretending the full
manifest and command-model move was already clean.

## Current State

- active strict lane: `g02.010`
- active ready card: `118`
- queued release card: `115`

`effigy-core` is now a real dependency, not planning-only scaffolding.

## Validation

- `cargo fmt --all`
- `cargo test -p effigy-core`
- `cargo test resolver_tests --lib`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `single-crate runtime with only planned backbone separation` to
  `Cargo workspace plus first shared backbone crate in live use`
- remains open:
  - manifest/core follow-up extraction
  - task-domain extraction
  - later release/distribution/container/demo extraction
  - eventual resume of `g02.007` release closure for `v0.3`

## Next Task

Execute
[`118-implement-effigy-tasks-foundation-extraction.md`](../../specs/batch-cards/118-implement-effigy-tasks-foundation-extraction.md)
to start the first domain extraction on top of `effigy-core`.
