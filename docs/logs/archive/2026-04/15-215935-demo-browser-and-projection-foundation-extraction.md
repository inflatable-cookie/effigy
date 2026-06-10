# Demo Browser And Projection Foundation Extraction

Date: 2026-04-15
Owner: Platform

## Summary

`129` is complete.

Effigy now has a shared browser/list/inspect payload contract under
[`crates/effigy-demo/src/browser.rs`](../../../../crates/effigy-demo/src/browser.rs).
The demo browser model no longer lives only inside
`src/runner/demo_command.rs` and `src/tui/demo_browser.rs`.

## What Changed

- widened [`effigy-demo`](../../../../crates/effigy-demo/Cargo.toml) with a shared
  browser payload module
- moved the shared browser-facing model there:
  - list payload
  - inspect payload
  - history payload
  - nested runtime, action, terminal, and latest-attempt browser projections
- reconnected [`src/runner/demo_command.rs`](../../../../src/runner/demo_command.rs)
  so demo list/inspect/history JSON now flows through the shared payload model
- reconnected [`src/tui/demo_browser.rs`](../../../../src/tui/demo_browser.rs)
  so the TUI consumes the shared payload model instead of a parallel local copy

## Why Runtime And Terminal Session Are Next

The demo domain still has one large live seam after this batch:

- [`src/runner/demo_command.rs`](../../../../src/runner/demo_command.rs)
- [`src/tui/demo_browser.rs`](../../../../src/tui/demo_browser.rs)

The remaining weight is now less about payload contracts and more about
runtime-backend, active-attempt, and terminal-session orchestration.

## Current State

- active strict lane: `g02.010`
- active ready card: `130`
- queued release card: `115`

## Validation

- `cargo test -p effigy-demo`
- `cargo test demo_command --lib`
- `cargo test --test cli_output_tests demo`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `runner- and tui-owned demo browser payload contracts`
  to `workspace-owned shared demo browser payload model with runner and TUI adapters`
- remains open:
  - demo runtime and terminal-session extraction
  - further modularization beyond the already-shipped crate slices
  - release closure and `v0.3` readiness through `g02.007` once the higher architectural bar is met

## Next Task

Execute
[`130-implement-demo-runtime-and-terminal-session-foundation-extraction.md`](../../../specs/batch-cards/130-implement-demo-runtime-and-terminal-session-foundation-extraction.md)
to widen `effigy-demo` around the remaining runtime and terminal-session cluster.
