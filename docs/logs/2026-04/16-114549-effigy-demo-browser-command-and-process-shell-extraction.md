# Effigy Demo Browser Command And Process Shell Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `157`

## Summary

Moved the browser demo-command contract into `crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- demo command request builders for list, inspect, history, input, stop, and
  resize
- refresh-load request shaping for the browser
- shared payload parsing for list, inspect, and history responses
- shared payload message extraction for browser command summaries

`src/tui/demo_browser.rs` now treats that layer as crate-owned and mainly
adapts the remaining host command execution, event loop, terminal lifecycle,
and root process shell.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser command request building and payload parsing still root-owned in src/tui`
  to `browser command/load bridge extracted into effigy-tui with src/tui narrowed toward host bridge and event loop ownership`
- Remains open: host command execution, terminal lifecycle, event loop wiring,
  and final root process shell still dominate the remaining
  `src/tui/demo_browser.rs`

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction to `1595` lines
- `crates/effigy-tui/src/demo_browser.rs` widened into command/load bridge
  ownership at `3247` lines
- focused browser and TUI validation stayed green after one crate dependency
  fix and stale root test-import rewires

## Next Task

Execute
`158-implement-effigy-demo-browser-host-bridge-and-event-loop-extraction.md`
to keep shrinking the remaining browser-local shell in
`src/tui/demo_browser.rs`.
