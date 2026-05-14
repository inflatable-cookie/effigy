# Runner-Private Domain Logic Closeout

Date: 2026-05-14
Roadmap: `g06.008`
Batch card: `808`

## Summary

Finished the runner-private state-domain reduction lane and cleared the last
warning-level god file from the codebase lean-down suite.

## Changes

- moved standalone/composed state manifest loading into
  [`crates/effigy-state/src/config.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-state/src/config.rs)
- moved named capture-profile request expansion and required-field validation
  into the same state-domain owner
- rewired
  [`src/runner/state_command.rs`](/Users/tom/Dev/projects/effigy/src/runner/state_command.rs)
  to use the new state-owned selectors and request resolver
- added state-crate tests for named capture-profile resolution and validation

## Outcome

- `src/runner/state_command.rs` dropped from `1918` lines at the start of
  `g06.008` to `1605`
- `effigy scan god-files --json` now reports `0` findings
- the remaining state runner surface is primarily CLI adaptation, execution
  dispatch, and rendering glue

## Vision Target Delta

- primary tags: `ROUTE`, `CONTRACT`, `MAINT`
- moved: runner-owned state config and capture-resolution policy ->
  `effigy-state` ownership, clearing the last warning-level oversized file
- remains open: `g06.001` final closeout proof in `809`

## Validation

- `cargo test -p effigy-state`
- `cargo test state_command`
- `cargo run --bin effigy -- scan god-files --json`
