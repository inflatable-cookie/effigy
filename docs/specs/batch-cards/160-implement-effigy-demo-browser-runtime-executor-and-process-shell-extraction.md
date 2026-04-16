# 160 Implement Effigy Demo Browser Runtime Executor And Process Shell Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next bounded demo-browser TUI slice so the remaining
`src/tui/demo_browser.rs` weight is no longer dominated by the runtime
executor, command bridge, and final process-facing shell ownership.

## In Scope

- classify the reusable browser runtime-execution contract
- move the next command-executor or terminal/process slice into
  `crates/effigy-tui` where it belongs
- reconnect the root crate through thinner runtime/process adapters
- leave any final OS/process shell explicit

## Out Of Scope

- full demo-browser decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- `crates/effigy-tui` widens beyond event-loop host-effect ownership
- `src/tui/demo_browser.rs` meaningfully shrinks or thins again
- the remaining browser-local shell is explicit after the batch

## Validation

- bounded demo-browser TUI validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`161-implement-effigy-demo-browser-terminal-bootstrap-and-runtime-boundary-extraction.md`
to keep shrinking the remaining browser-local shell in
`src/tui/demo_browser.rs`.
