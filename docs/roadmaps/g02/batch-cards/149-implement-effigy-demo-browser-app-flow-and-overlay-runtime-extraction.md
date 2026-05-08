# 149 Implement Effigy Demo Browser App Flow And Overlay Runtime Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next bounded demo-browser TUI slice so the remaining
`src/tui/demo_browser.rs` weight is no longer dominated by browser app-flow,
overlay, and selection/runtime coordination.

## In Scope

- classify the reusable browser app-flow and overlay/runtime contracts
- move that slice into `crates/effigy-tui`
- reconnect the root crate through thinner adapters
- leave the remaining command bridge and final shell wiring explicit

## Out Of Scope

- full demo-browser decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- `crates/effigy-tui` widens beyond terminal/live-session handling
- `src/tui/demo_browser.rs` meaningfully shrinks or thins again
- the remaining browser-local shell is explicit after the batch

## Validation

- bounded demo-browser TUI validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`150-implement-effigy-demo-browser-state-machine-and-command-bridge-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
