# Effigy Demo Browser Runtime Command Bridge Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `154`

## Summary

Moved the browser runtime-command planning layer into `crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- run/rerun planning and background/live-launch decision shaping
- stop command planning
- artifact-open planning
- retained-history refresh planning
- selected-detail entry dispatch planning
- background command registration ownership

`src/tui/demo_browser.rs` now treats that layer as crate-owned and mainly
adapts the remaining refresh/load bridge, poll lifecycle, and top-level browser
runtime loop.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser runtime command bridge and selected-detail dispatch still root-owned in src/tui`
  to `browser runtime-command planning extracted into effigy-tui with src/tui narrowed toward refresh/poll lifecycle and browser run-loop wiring`
- Remains open: refresh/load bridging, poll lifecycle wiring, and the top-level
  browser runtime loop still dominate the remaining
  `src/tui/demo_browser.rs` shell

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction to `1873` lines
- `crates/effigy-tui/src/demo_browser.rs` widened into runtime-command planning
  ownership at `2878` lines
- the browser/TUI validation round stayed green after the move

## Next Task

Execute
`155-implement-effigy-demo-browser-refresh-lifecycle-and-run-loop-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
