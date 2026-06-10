# Effigy Demo Browser Event Loop And Terminal Shell Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `152`

## Summary

Moved the next browser-local TUI shell slice into `crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- browser escape handling outcome classification
- demo-list and detail-panel up/down navigation handling
- selected live-session lookup on browser state
- terminal panel rendering and viewport-state updates

`src/tui/demo_browser.rs` now treats that slice as crate-owned and mainly
adapts overlay handling, runner-command effects, and the top-level browser
loop.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser event/runtime shell and terminal panel wiring still root-owned in src/tui`
  to `browser navigation and terminal panel shell extracted into effigy-tui with src/tui narrowed toward overlay handling and runner-command bridge effects`
- Remains open: overlay key handling, runner-command bridge effects, and the
  top-level browser loop still dominate the remaining
  `src/tui/demo_browser.rs` shell

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction to `2189` lines
- `crates/effigy-tui/src/demo_browser.rs` widened into navigation and terminal
  panel ownership at `2427` lines
- the browser/TUI validation round stayed green after the move

## Next Task

Execute
`153-implement-effigy-demo-browser-runner-bridge-and-overlay-loop-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
