# Runner State Domain Helper Extraction

Date: 2026-05-14
Roadmap: `g06.008`
Batch card: `808`

## Summary

Moved another block of durable state behavior out of the runner shell and into
`effigy-state`, while keeping runner error messages and state command behavior
stable.

## Changes

- moved state report writing and context-file writing into
  [`crates/effigy-state/src/paths.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-state/src/paths.rs)
- moved apply-hook env construction and skip-layer validation into
  [`crates/effigy-state/src/apply.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-state/src/apply.rs)
- moved capture-task env construction into
  [`crates/effigy-state/src/capture.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-state/src/capture.rs)
- moved history-kind parsing into
  [`crates/effigy-state/src/history.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-state/src/history.rs)
- rewired
  [`src/runner/state_command.rs`](/Users/tom/Dev/projects/effigy/src/runner/state_command.rs)
  to use the state-owned helpers with thin `RunnerError` adapters
- added state-crate tests for:
  - report writing
  - context writing
  - apply/capture env helpers
  - skip-layer validation
  - history-kind parsing

## Outcome

- `src/runner/state_command.rs` dropped from `1918` lines to `1726`
- `effigy scan god-files --json` now reports one remaining warning file:
  `src/runner/state_command.rs`
- the moved behavior now has a clearer state-domain owner and direct crate
  tests instead of living only behind runner tests

## Vision Target Delta

- primary tags: `ROUTE`, `CONTRACT`, `MAINT`
- moved: runner-owned state helper logic -> `effigy-state` ownership with
  focused crate tests and a thinner runner shell
- remains open: `g06.008` still needs at least one more durable extraction
  slice before `state_command.rs` can be considered justified or close-ready

## Validation

- `cargo test -p effigy-state`
- `cargo test state_command`
- `cargo run --bin effigy -- scan god-files --json`
