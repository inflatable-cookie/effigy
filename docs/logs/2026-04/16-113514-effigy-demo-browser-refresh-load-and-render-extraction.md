# Effigy Demo Browser Refresh Load And Render Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `156`

## Summary

Moved the browser refresh/load planning and non-terminal render shell into
`crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- refresh-load planning for selected detail and retained-history fetches
- detail-tab state application for browser-local tab transitions
- the browser screen render shell
- the browser body/detail render shell for non-terminal views

`src/tui/demo_browser.rs` now treats that layer as crate-owned and mainly
adapts the remaining command execution, JSON/load bridge, terminal/process
wiring, and root event loop.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser refresh-load planning and non-terminal render shell still root-owned in src/tui`
  to `browser refresh-load planning and non-terminal render shell extracted into effigy-tui with src/tui narrowed toward command/process bridge ownership`
- Remains open: command execution, JSON/load bridging, terminal/process wiring,
  and final root event loop shell still dominate the remaining
  `src/tui/demo_browser.rs`

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction to `1686` lines
- `crates/effigy-tui/src/demo_browser.rs` widened into refresh/load and render
  ownership at `3104` lines
- focused browser and TUI validation stayed green after two small root-side
  import/borrow fixes

## Next Task

Execute
`157-implement-effigy-demo-browser-command-and-process-shell-extraction.md`
to keep shrinking the remaining browser-local shell in
`src/tui/demo_browser.rs`.
