# Effigy Demo Foundation Extraction

Date: 2026-04-15
Owner: Platform

## Summary

`128` is complete.

Effigy now has a real `effigy-demo` workspace crate. Demo receipt, path, and
attempt-history ownership no longer sit entirely inside
`src/runner/demo_command.rs`.

## What Changed

- added [`crates/effigy-demo`](../../../../crates/effigy-demo/Cargo.toml)
- moved the first reusable demo state boundary there:
  - receipt/path helpers
  - attempt-id and repo-path helpers
  - latest-attempt loading
  - attempt-history loading and persistence
  - persisted and projected attempt-history contracts
- reconnected [`src/runner/demo_command.rs`](../../../../src/runner/demo_command.rs)
  as a thinner adapter over that state boundary
- added focused crate coverage for:
  - attempt-history retention
  - latest-attempt receipt loading
  - attempt-history append/load flow

## Why Browser And Projection Are Next

The demo domain is still the largest interleaved cluster after this batch:

- [`src/runner/demo_command.rs`](../../../../src/runner/demo_command.rs)
- [`src/tui/demo_browser.rs`](../../../../src/tui/demo_browser.rs)

The next honest seam is the browser/projection layer that both CLI and TUI
still depend on.

## Current State

- active strict lane: `g02.010`
- active ready card: `129`
- queued release card: `115`

## Validation

- `cargo test -p effigy-demo`
- `cargo test demo_command --lib`
- `cargo test --test cli_output_tests demo`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `runner-owned demo receipt/history/path state`
  to `workspace-owned demo state/history foundation with runner adapters`
- remains open:
  - demo browser/projection extraction
  - further modularization beyond the already-shipped crate slices
  - release closure and `v0.3` readiness through `g02.007` once the higher architectural bar is met

## Next Task

Execute
[`129-implement-demo-browser-and-projection-foundation-extraction.md`](../../../specs/batch-cards/129-implement-demo-browser-and-projection-foundation-extraction.md)
to widen `effigy-demo` around the remaining browser/projection cluster.
