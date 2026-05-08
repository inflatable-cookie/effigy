# 157 Implement Effigy Demo Browser Command And Process Shell Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next bounded demo-browser TUI slice so the remaining
`src/tui/demo_browser.rs` weight is no longer dominated by browser command
execution, JSON/load bridging, and process-facing shell ownership.

## In Scope

- classify the reusable browser command/load bridge contract
- move the next browser command/process slice into `crates/effigy-tui` where it
  belongs
- reconnect the root crate through thinner runner/process adapters
- leave any final OS/process shell explicit

## Out Of Scope

- full demo-browser decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- `crates/effigy-tui` widens beyond refresh/load and render ownership
- `src/tui/demo_browser.rs` meaningfully shrinks or thins again
- the remaining browser-local shell is explicit after the batch

## Validation

- bounded demo-browser TUI validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`158-implement-effigy-demo-browser-host-bridge-and-event-loop-extraction.md`
to keep shrinking the remaining browser-local shell in
`src/tui/demo_browser.rs`.
