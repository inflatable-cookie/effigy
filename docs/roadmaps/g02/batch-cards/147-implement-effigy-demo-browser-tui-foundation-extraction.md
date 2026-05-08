# 147 Implement Effigy Demo Browser TUI Foundation Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next bounded browser-local TUI slice so `src/tui/demo_browser.rs`
stops dominating the remaining shell weight.

## In Scope

- classify the first reusable demo-browser TUI contracts
- move that slice into `crates/effigy-tui`
- reconnect the root crate through thin adapters
- leave the remaining browser-only shell explicit

## Out Of Scope

- full demo-browser decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- `crates/effigy-tui` widens beyond multiprocess foundation contracts
- `src/tui/demo_browser.rs` meaningfully shrinks or thins
- the remaining browser-only shell is explicit after the batch

## Validation

- bounded demo-browser TUI validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `148-implement-effigy-demo-browser-terminal-and-live-session-extraction.md`
to shrink the remaining terminal/session shell in `src/tui/demo_browser.rs`.
