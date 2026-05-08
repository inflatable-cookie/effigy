# 155 Implement Effigy Demo Browser Refresh Lifecycle And Run Loop Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next bounded demo-browser TUI slice so the remaining
`src/tui/demo_browser.rs` weight is no longer dominated by poll lifecycle
wiring, refresh cadence ownership, and the top-level browser run loop.

## In Scope

- classify the reusable browser refresh lifecycle and run-loop contract
- move that slice into `crates/effigy-tui`
- reconnect the root crate through thinner runner/process adapters
- leave any final OS/process shell explicit

## Out Of Scope

- full demo-browser decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- `crates/effigy-tui` widens beyond runtime-command planning ownership
- `src/tui/demo_browser.rs` meaningfully shrinks or thins again
- the remaining browser-local shell is explicit after the batch

## Validation

- bounded demo-browser TUI validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`156-implement-effigy-demo-browser-refresh-load-and-render-extraction.md`
to keep shrinking the remaining browser-local shell in
`src/tui/demo_browser.rs`.
