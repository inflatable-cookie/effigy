# Effigy Demo Browser Refresh Lifecycle And Run Loop Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `155`

## Summary

Moved the browser lifecycle polling and run-loop timing seam into
`crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- runtime lifecycle event classification for pending actions and live terminal
  sessions
- polling helpers for pending-action and live-session completion
- pending live-terminal launch take/restore helpers
- live terminal session registration shaping
- browser auto-refresh cadence ownership

`src/tui/demo_browser.rs` now treats that layer as crate-owned and mainly
adapts the remaining refresh/load bridge, render shell, and OS/process wiring.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser refresh cadence, lifecycle polling, and run-loop timing still root-owned in src/tui`
  to `browser lifecycle polling and run-loop timing extracted into effigy-tui with src/tui narrowed toward refresh/load bridge and render shell ownership`
- Remains open: refresh/load bridging, detail/history fetch shaping, render
  shell, and final OS/process wiring still dominate the remaining
  `src/tui/demo_browser.rs` shell

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction to `1848` lines
- `crates/effigy-tui/src/demo_browser.rs` widened into lifecycle/run-loop
  ownership at `2950` lines
- focused browser and TUI validation stayed green after fixing one stale test
  import

## Next Task

Execute
`156-implement-effigy-demo-browser-refresh-load-and-render-extraction.md`
to keep shrinking the remaining browser-local shell in
`src/tui/demo_browser.rs`.
