# Effigy Demo Browser Host Bridge And Event Loop Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `158`

## Summary

Moved the browser host request shaping further into `crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- action-menu planning for refresh versus runtime commands
- terminal input-mode toggle behavior
- forwarded terminal input request shaping
- resize request shaping and applied-size tracking
- live-terminal shutdown request shaping

`src/tui/demo_browser.rs` now treats that layer as crate-owned and mainly
adapts the remaining root event loop, command execution bridge, and final
terminal/process shell.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser host request shaping still split between effigy-tui and src/tui`
  to `browser interaction and host request shaping extracted further into effigy-tui with src/tui narrowed toward event loop and final process shell ownership`
- Remains open: root event loop, command execution bridge, artifact opener, and
  terminal/process lifecycle still dominate the remaining
  `src/tui/demo_browser.rs`

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction to `1530` lines
- `crates/effigy-tui/src/demo_browser.rs` widened into host request shaping at
  `3366` lines
- focused browser and TUI validation stayed green after two small root test
  import/borrow fixes

## Next Task

Execute
`159-implement-effigy-demo-browser-event-loop-and-terminal-process-shell-extraction.md`
to keep shrinking the remaining browser-local shell in
`src/tui/demo_browser.rs`.
