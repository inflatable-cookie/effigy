# Effigy Demo Browser Effect Loop And Runner Bridge Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `151`

## Summary

Moved the next browser runtime slice into `crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- browser refresh projection application
- selected-demo resolution for refreshed row sets
- pending action completion and disconnect handling
- live terminal completion handling

`src/tui/demo_browser.rs` now treats that refresh/poll state application as
crate-owned and mainly adapts the remaining event loop, terminal/runtime shell,
and top-level runner bridge effects.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser refresh/poll orchestration and runner-bridge state application still root-owned in src/tui`
  to `browser refresh projection and pending result handling extracted into effigy-tui with src/tui narrowed toward event-loop and terminal/runtime shell wiring`
- Remains open: browser event-loop handling, terminal/runtime shell wiring, and
  top-level runner bridge effects still dominate the remaining
  `src/tui/demo_browser.rs` shell

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction to `2337` lines
- `crates/effigy-tui/src/demo_browser.rs` widened into refresh/poll projection
  ownership at `2281` lines
- the browser/TUI validation round stayed green after the move

## Next Task

Execute
`152-implement-effigy-demo-browser-event-loop-and-terminal-shell-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
