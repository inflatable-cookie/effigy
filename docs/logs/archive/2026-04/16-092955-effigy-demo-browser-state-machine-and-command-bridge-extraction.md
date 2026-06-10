# Effigy Demo Browser State Machine And Command Bridge Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `150`

## Summary

Moved the demo-browser owned state model into `crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- the browser state struct and its lifecycle defaults
- selection, focus, overlay, and detail-navigation state helpers
- browser-owned detail-render and item-selection contracts
- state-side pending action and pending launch ownership

`src/tui/demo_browser.rs` now treats that state as crate-owned and mainly
adapts runner-facing effects around it.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser state machine still root-owned in src/tui` to
  `browser state model extracted into effigy-tui with src/tui narrowed toward
  effect-loop and runner bridge wiring`
- Remains open: browser effect-loop handling, refresh/poll orchestration, and
  runner-command bridge wiring still dominate the remaining
  `src/tui/demo_browser.rs` shell

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction
- `crates/effigy-tui/src/demo_browser.rs` widened into browser state ownership
- the browser/TUI validation round stayed green after the move

## Next Task

Execute
`151-implement-effigy-demo-browser-effect-loop-and-runner-bridge-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
