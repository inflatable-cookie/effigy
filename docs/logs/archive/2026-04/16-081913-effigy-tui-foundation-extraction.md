# Effigy TUI Foundation Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Card: `145`

## Summary

The first TUI shell seam now lives in a real workspace crate.

`crates/effigy-tui` now owns the shared TUI core contracts plus the
multiprocess terminal-text runtime helpers. That removes those primitives from
`src/tui` and leaves the remaining weight concentrated in the browser file and
the wider multiprocess runtime tree.

## What Changed

- added `crates/effigy-tui`
- moved shared TUI core contracts into `crates/effigy-tui/src/core.rs`
- moved multiprocess terminal-text runtime helpers into
  `crates/effigy-tui/src/terminal_text/`
- rewired `src/tui/core.rs` and `src/tui/multiprocess/terminal_text/mod.rs`
  into thin compatibility adapters
- removed the duplicated parser constants from
  `src/tui/multiprocess/config.rs`

## Boundary Result

The first TUI slice is no longer ambiguous.

What remains is now clearer:

- `src/tui/demo_browser.rs` as the dominant browser-local shell
- the supporting multiprocess runtime tree as the next reusable TUI seam

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: `shared TUI core and terminal-text runtime inline in src/tui` -> `shared TUI core and terminal-text runtime owned by crates/effigy-tui`
- Remaining gap: `multiprocess TUI/runtime extraction before release closure can resume honestly`

## Validation Performed

- command: `cargo fmt --all`
  - result: passed
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

Execute [`146-implement-effigy-multiprocess-tui-foundation-extraction.md`](../../../specs/batch-cards/146-implement-effigy-multiprocess-tui-foundation-extraction.md).
