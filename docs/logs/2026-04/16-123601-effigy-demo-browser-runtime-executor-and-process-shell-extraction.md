# Effigy Demo Browser Runtime Executor And Process Shell Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `160`

## Summary

Moved the browser loop polling and artifact-opening helpers into
`crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- browser loop event polling and event classification
- artifact opener execution and platform command shaping
- the shared process-facing shell for browser artifact opening

`src/tui/demo_browser.rs` now treats that layer as crate-owned and mainly
adapts the remaining runtime executor, command invocation bridge, terminal
bootstrap, and final process shell.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser loop polling and artifact-open process shell still root-owned in src/tui`
  to `browser loop polling and artifact-open process shell extracted into effigy-tui with src/tui narrowed toward runtime executor and terminal bootstrap ownership`
- Remains open: runtime execution, terminal bootstrap, command invocation, and
  final process shell still dominate the remaining `src/tui/demo_browser.rs`

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction to `1456` lines
- `crates/effigy-tui/src/demo_browser.rs` widened into loop/process helper
  ownership at `3484` lines
- focused browser and TUI validation stayed green after one stale root test
  import fix

## Next Task

Execute
`161-implement-effigy-demo-browser-terminal-bootstrap-and-runtime-boundary-extraction.md`
to keep shrinking the remaining browser-local shell in
`src/tui/demo_browser.rs`.
