# 161 Implement Effigy Demo Browser Terminal Bootstrap And Runtime Boundary Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next bounded demo-browser TUI slice so the remaining
`src/tui/demo_browser.rs` weight is no longer dominated by terminal bootstrap,
runtime-command execution, and the final process-facing shell ownership.

## In Scope

- classify the reusable browser runtime-command execution contract
- move the next terminal bootstrap or runtime-executor slice into
  `crates/effigy-tui` where it belongs
- reconnect the root crate through thinner runtime/process adapters
- leave any final OS/process shell explicit

## Out Of Scope

- full demo-browser decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- `crates/effigy-tui` widens beyond event-loop and process-helper ownership
- `src/tui/demo_browser.rs` meaningfully shrinks or thins again
- the remaining browser-local shell is explicit after the batch

## Validation

- bounded demo-browser TUI validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`162-decide-post-demo-browser-runtime-boundary.md`
to decide whether the remaining browser shell is now honest adapter/runtime
work or still needs one more bounded extraction batch.
