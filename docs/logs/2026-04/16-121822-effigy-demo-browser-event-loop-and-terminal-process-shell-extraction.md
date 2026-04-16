# Effigy Demo Browser Event Loop And Terminal Process Shell Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `159`

## Summary

Moved the browser host-effect bridge further into `crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- host-effect resolution from key actions
- detail-tab request planning through the host-effect bridge
- forwarded terminal-input host effects
- action-menu runtime versus refresh effect resolution

`src/tui/demo_browser.rs` now treats that layer as crate-owned and mainly
adapts the remaining root event loop, runtime executor, command bridge, and
final process shell.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser host interaction still split between event loop dispatch and effigy-tui state helpers`
  to `browser host-effect resolution extracted into effigy-tui with src/tui narrowed toward runtime executor and process shell ownership`
- Remains open: runtime execution, command invocation, artifact opening, and
  final terminal/process shell still dominate the remaining
  `src/tui/demo_browser.rs`

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction to `1499` lines
- `crates/effigy-tui/src/demo_browser.rs` widened into host-effect resolution
  at `3427` lines
- focused browser and TUI validation stayed green after one test-wrapper borrow
  fix

## Next Task

Execute
`160-implement-effigy-demo-browser-runtime-executor-and-process-shell-extraction.md`
to keep shrinking the remaining browser-local shell in
`src/tui/demo_browser.rs`.
