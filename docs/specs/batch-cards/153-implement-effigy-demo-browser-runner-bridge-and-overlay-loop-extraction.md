# 153 Implement Effigy Demo Browser Runner Bridge And Overlay Loop Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next bounded demo-browser TUI slice so the remaining
`src/tui/demo_browser.rs` weight is no longer dominated by overlay key
handling, runner-command bridge effects, and the top-level browser loop.

## In Scope

- classify the reusable browser overlay-loop and runner-bridge contract
- move that slice into `crates/effigy-tui`
- reconnect the root crate through thinner browser command/runtime adapters
- leave any final OS/process shell explicit

## Out Of Scope

- full demo-browser decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- `crates/effigy-tui` widens beyond terminal panel and navigation ownership
- `src/tui/demo_browser.rs` meaningfully shrinks or thins again
- the remaining browser-local shell is explicit after the batch

## Validation

- bounded demo-browser TUI validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`154-implement-effigy-demo-browser-runtime-command-bridge-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
