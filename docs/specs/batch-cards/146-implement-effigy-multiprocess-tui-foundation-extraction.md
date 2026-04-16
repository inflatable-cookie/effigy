# 146 Implement Effigy Multiprocess TUI Foundation Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next bounded TUI runtime seam so the remaining `src/tui` weight is
not mostly the multiprocess session stack plus one giant browser file.

## In Scope

- classify the first reusable multiprocess TUI/runtime contracts
- move that slice into `crates/effigy-tui`
- reconnect the root crate through thin adapters
- leave the remaining browser-local shell explicit

## Out Of Scope

- full `demo_browser.rs` decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- `crates/effigy-tui` widens beyond core plus terminal text
- the multiprocess runtime tree in `src/tui` meaningfully shrinks or thins
- the remaining browser-local shell is explicit after the batch

## Validation

- bounded TUI/runtime validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `147-implement-effigy-demo-browser-tui-foundation-extraction.md` to
shrink the remaining browser-local TUI shell.
