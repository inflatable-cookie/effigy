# Effigy Multiprocess TUI Foundation Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Card: `146`

## Summary

The next multiprocess TUI runtime slice now lives in `crates/effigy-tui`.

`effigy-tui` now owns multiprocess config, diagnostics, session state, and
view-model building. That cuts the reusable runtime model out of
`src/tui/multiprocess` and leaves the remaining local shell centered on event
handling, rendering, lifecycle wiring, and the still-large demo browser file.

## What Changed

- widened `crates/effigy-tui` with `multiprocess/`
- moved multiprocess config constants into the crate
- moved runtime diagnostics into the crate
- moved session-state ownership and accessors into the crate
- moved active view-model and scroll-building logic into the crate
- rewired `src/tui/multiprocess/{config,diagnostics,state,view_model}` into
  thin compatibility adapters

## Boundary Result

The reusable multiprocess runtime model is no longer inline shell residue.

What remains is clearer:

- browser-local TUI behavior in `src/tui/demo_browser.rs`
- multiprocess event/render/lifecycle shell wiring in `src/tui/multiprocess`

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: `multiprocess runtime config/state/view-model inline in src/tui` -> `multiprocess runtime config/state/view-model owned by crates/effigy-tui`
- Remaining gap: `demo-browser TUI extraction before release closure can resume honestly`

## Validation Performed

- command: `cargo test -p effigy-tui`
  - result: passed
- command: `cargo test multiprocess --lib`
  - result: passed
- command: `cargo test demo_browser --lib`
  - result: passed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Next Task

Execute [`147-implement-effigy-demo-browser-tui-foundation-extraction.md`](../../../specs/batch-cards/147-implement-effigy-demo-browser-tui-foundation-extraction.md).
