# Effigy Demo Browser Terminal And Live Session Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `148`

## Summary

Moved the demo-browser terminal-view and live-session runtime slice into
`crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- terminal viewport and status rendering helpers
- recent-log loading for terminal replay
- live browser terminal session spawn, polling, input, and shutdown helpers
- terminal transcript sanitizing and split-byte buffering helpers
- browser live-terminal environment shaping

`src/tui/demo_browser.rs` now consumes those extracted contracts instead of
owning them inline.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser terminal/session runtime still inline in src/tui` to
  `browser terminal/session runtime extracted into effigy-tui with src/tui as
  adapter`
- Remains open: browser app-flow, overlay, and top-level event/runtime wiring
  still dominate the remaining `src/tui/demo_browser.rs` shell

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction and now leaves
  the remaining browser-local shell more explicit
- `crates/effigy-tui/src/demo_browser.rs` widened beyond browser
  presentation/state into terminal and live-session ownership
- the main crate still passes the focused browser/TUI validation round after
  the move

## Next Task

Execute
`149-implement-effigy-demo-browser-app-flow-and-overlay-runtime-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
