# Effigy Demo Browser Runner Bridge And Overlay Loop Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `153`

## Summary

Moved the browser key-decision and overlay-loop layer into `crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- top-level browser key routing
- terminal-input key interpretation
- left/right/enter browser action routing
- overlay prompt/action/filter key handling
- overlay-driven query mutation and refresh intent classification

`src/tui/demo_browser.rs` now treats that logic as crate-owned and mainly
adapts the remaining runner-command effects, refresh/poll lifecycle, and
browser runtime orchestration.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser overlay loop and key-to-action routing still root-owned in src/tui`
  to `browser key routing and overlay-loop decisions extracted into effigy-tui with src/tui narrowed toward runner-command and runtime lifecycle effects`
- Remains open: runner-command effects, refresh/poll lifecycle wiring, and the
  top-level browser runtime loop still dominate the remaining
  `src/tui/demo_browser.rs` shell

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction to `1965` lines
- `crates/effigy-tui/src/demo_browser.rs` widened into key and overlay routing
  ownership at `2715` lines
- the browser/TUI validation round stayed green after the move

## Next Task

Execute
`154-implement-effigy-demo-browser-runtime-command-bridge-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
