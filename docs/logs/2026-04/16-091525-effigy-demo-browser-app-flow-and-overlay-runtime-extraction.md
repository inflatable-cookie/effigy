# Effigy Demo Browser App Flow And Overlay Runtime Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `149`

## Summary

Moved the browser app-flow render shell and overlay rendering layer into
`crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- browser header/list/footer rendering
- empty-state overlay rendering
- prompt/action/filter overlay rendering
- shared overlay and pending-launch state contracts

`src/tui/demo_browser.rs` now consumes those extracted surfaces instead of
owning that UI layer inline.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser app-flow still mixed with render and overlay shell in src/tui`
  to `browser render and overlay shell extracted into effigy-tui with src/tui
  narrowed toward command/event wiring`
- Remains open: browser state machine flow, selection/runtime coordination, and
  command-bridge effect handling still dominate the remaining
  `src/tui/demo_browser.rs` shell

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction
- `crates/effigy-tui/src/demo_browser.rs` widened into browser render and
  overlay ownership
- the browser/TUI validation round stayed green after the move

## Next Task

Execute
`150-implement-effigy-demo-browser-state-machine-and-command-bridge-extraction.md`
to keep shrinking the remaining browser-local shell in `src/tui/demo_browser.rs`.
