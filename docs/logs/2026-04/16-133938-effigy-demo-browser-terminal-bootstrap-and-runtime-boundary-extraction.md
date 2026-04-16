# Effigy Demo Browser Terminal Bootstrap And Runtime Boundary Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Batch: `161`

## Summary

Moved the browser terminal bootstrap and generic runtime-boundary helpers into
`crates/effigy-tui`.

`effigy-tui::demo_browser` now owns:

- browser terminal init and restore helpers
- generic refresh-through-executor orchestration
- generic detail-tab history loading through an injected executor
- generic resize application through an injected executor
- runtime-plan to execution-request mapping

`src/tui/demo_browser.rs` now treats that layer as crate-owned and mainly
adapts the remaining Effigy command invocation bridge plus the final
runtime/process shell around the browser.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `OPERATE`
- Moved from `browser terminal bootstrap and generic runtime-boundary helpers still root-owned in src/tui`
  to `browser terminal bootstrap and runtime-boundary helpers extracted into effigy-tui with src/tui narrowed toward the final Effigy runtime adapter shell`
- Remains open: the remaining browser root shell still needs an explicit
  boundary decision before pause or further widening

## Evidence

- `src/tui/demo_browser.rs` reduced again after the extraction to `1387` lines
- `crates/effigy-tui/src/demo_browser.rs` widened into terminal bootstrap and
  generic runtime-boundary ownership at `3633` lines
- focused browser and TUI validation stayed green after one small root import
  cleanup

## Next Task

Execute
`162-decide-post-demo-browser-runtime-boundary.md`
to decide whether the remaining browser shell is now honest adapter/runtime
work or still needs one more bounded extraction batch.
