# Demo Runtime And Terminal Session Foundation Extraction

Date: 2026-04-15
Owner: Platform

## Summary

`130` is complete.

Effigy now has a shared demo runtime and terminal-session contract under
[`crates/effigy-demo/src/runtime.rs`](../../../crates/effigy-demo/src/runtime.rs).
The active-attempt, runtime-backend, and terminal-session model no longer live
only inside [`src/runner/demo_command.rs`](../../../src/runner/demo_command.rs).

## What Changed

- widened [`effigy-demo`](../../../crates/effigy-demo/Cargo.toml) with a shared
  runtime module
- moved the shared runtime/session model there:
  - runtime backend and capability rendering
  - runtime projection shape
  - projected process summary
  - projected output provenance
  - active-attempt projection
  - active terminal session, input, resize, size, and recent-output contracts
- reconnected [`src/runner/demo_command.rs`](../../../src/runner/demo_command.rs)
  so demo inspect/history/execute/input/resize flows now use the shared
  runtime/session model
- kept browser-facing payload ownership in
  [`crates/effigy-demo/src/browser.rs`](../../../crates/effigy-demo/src/browser.rs)
  while reusing the shared runtime/session contracts there too

## Why A Boundary Decision Is Next

The remaining demo seam is smaller, but it is not yet obvious whether another
`effigy-demo` extraction is still the right move.

What remains is more shell-shaped:

- launch, stop, rerun, and concurrent-runner orchestration in
  [`src/runner/demo_command.rs`](../../../src/runner/demo_command.rs)
- browser-local live terminal session driving and rendering behavior in
  [`src/tui/demo_browser.rs`](../../../src/tui/demo_browser.rs)

That needs an explicit boundary decision instead of another guessed slice.

## Current State

- active strict lane: `g02.010`
- active ready card: `131`
- queued release card: `115`

## Validation

- `cargo fmt --all --check`
- `cargo test -p effigy-demo`
- `cargo test demo_command --lib`
- `cargo test --test cli_output_tests demo`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `runner-owned demo runtime and terminal-session model`
  to `workspace-owned shared demo runtime/session contract with runner and browser adapters`
- remains open:
  - post-demo boundary classification for the remaining shell
  - further modularization beyond the already-shipped crate slices
  - release closure and `v0.3` readiness through `g02.007` once the higher architectural bar is met

## Next Task

Execute
[`131-decide-post-demo-runtime-and-terminal-session-extraction-boundary.md`](../../specs/batch-cards/131-decide-post-demo-runtime-and-terminal-session-extraction-boundary.md)
to classify the remaining demo shell before modularization jumps to another
domain.
