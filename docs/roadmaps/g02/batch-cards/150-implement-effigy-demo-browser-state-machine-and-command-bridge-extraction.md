# 150 Implement Effigy Demo Browser State Machine And Command Bridge Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next bounded demo-browser TUI slice so the remaining
`src/tui/demo_browser.rs` weight is no longer dominated by browser state
machine flow, selection/runtime coordination, and command-bridge effect
handling.

## In Scope

- classify the reusable browser state machine and effect contract
- move that slice into `crates/effigy-tui`
- reconnect the root crate through thinner browser-command adapters
- leave any final runner-owned command invocation shell explicit

## Out Of Scope

- full demo-browser decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- `crates/effigy-tui` widens beyond render and terminal/live-session ownership
- `src/tui/demo_browser.rs` meaningfully shrinks or thins again
- the remaining browser-local shell is explicit after the batch

## Validation

- bounded demo-browser TUI validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`151-implement-effigy-demo-browser-effect-loop-and-runner-bridge-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
