# Effigy Demo Browser TUI Foundation Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Card: `147`

## Summary

The demo-browser presentation layer now lives in `crates/effigy-tui`.

`effigy-tui::demo_browser` now owns the browser row model, focus and overlay
state contracts, filter and prompt state, tab and detail selection contracts,
query/filter cycling helpers, and the pure detail/browser render helpers. That
cuts a real browser-facing slice out of `src/tui/demo_browser.rs` without
pretending the live terminal/session runtime is done.

## What Changed

- added `crates/effigy-tui/src/demo_browser.rs`
- moved browser row, focus, menu, tab, selection, filter, and prompt types
  into the crate
- moved browser query/filter helper functions into the crate
- moved browser detail render helpers into the crate
- moved browser list/header/help/presentation helpers into the crate
- rewired `src/tui/demo_browser.rs` to consume that extracted browser TUI
  surface

## Boundary Result

The demo-browser presentation/state layer is no longer inline shell residue.

What remains is clearer:

- browser app flow and top-level event handling
- terminal-view rendering and log sourcing
- live terminal session launch, IO, and shutdown handling

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: `demo-browser presentation and state contracts inline in src/tui/demo_browser.rs` -> `demo-browser presentation and state contracts owned by crates/effigy-tui`
- Remaining gap: `demo-browser terminal and live-session extraction before release closure can resume honestly`

## Validation Performed

- command: `cargo test -p effigy-tui`
  - result: passed
- command: `cargo test demo_browser --lib`
  - result: passed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Next Task

Execute [`148-implement-effigy-demo-browser-terminal-and-live-session-extraction.md`](../../../specs/batch-cards/148-implement-effigy-demo-browser-terminal-and-live-session-extraction.md).
