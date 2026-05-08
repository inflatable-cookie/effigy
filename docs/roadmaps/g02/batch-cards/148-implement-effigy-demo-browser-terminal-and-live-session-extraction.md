# 148 Implement Effigy Demo Browser Terminal And Live Session Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next bounded demo-browser TUI slice so the remaining
`src/tui/demo_browser.rs` weight is no longer dominated by terminal-view and
live-session runtime handling.

## In Scope

- classify the reusable browser terminal and live-session contracts
- move that slice into `crates/effigy-tui`
- reconnect the root crate through thin adapters
- leave the remaining browser-local shell explicit

## Out Of Scope

- full demo-browser decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- `crates/effigy-tui` widens beyond browser presentation/state contracts
- `src/tui/demo_browser.rs` meaningfully shrinks or thins again
- the remaining browser-local shell is explicit after the batch

## Validation

- bounded demo-browser TUI validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`149-implement-effigy-demo-browser-app-flow-and-overlay-runtime-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
