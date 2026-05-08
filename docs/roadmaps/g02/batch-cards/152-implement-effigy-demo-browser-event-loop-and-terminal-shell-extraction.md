# 152 Implement Effigy Demo Browser Event Loop And Terminal Shell Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next bounded demo-browser TUI slice so the remaining
`src/tui/demo_browser.rs` weight is no longer dominated by browser event-loop
handling, terminal/runtime shell wiring, and top-level runner bridge effects.

## In Scope

- classify the reusable browser event-loop and terminal shell contract
- move that slice into `crates/effigy-tui`
- reconnect the root crate through thinner browser runtime adapters
- leave any final OS/process shell explicit

## Out Of Scope

- full demo-browser decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- `crates/effigy-tui` widens beyond refresh/poll projection ownership
- `src/tui/demo_browser.rs` meaningfully shrinks or thins again
- the remaining browser-local shell is explicit after the batch

## Validation

- bounded demo-browser TUI validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`153-implement-effigy-demo-browser-runner-bridge-and-overlay-loop-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
