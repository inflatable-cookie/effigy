# 151 Implement Effigy Demo Browser Effect Loop And Runner Bridge Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next bounded demo-browser TUI slice so the remaining
`src/tui/demo_browser.rs` weight is no longer dominated by browser effect-loop
handling, refresh/poll orchestration, and runner-command bridge wiring.

## In Scope

- classify the reusable browser effect-loop and bridge contract
- move that slice into `crates/effigy-tui` where it belongs
- reconnect the root crate through thinner runner-command adapters
- leave any final OS/process shell explicit

## Out Of Scope

- full demo-browser decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- `crates/effigy-tui` widens beyond browser state ownership
- `src/tui/demo_browser.rs` meaningfully shrinks or thins again
- the remaining browser-local shell is explicit after the batch

## Validation

- bounded demo-browser TUI validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`152-implement-effigy-demo-browser-event-loop-and-terminal-shell-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
