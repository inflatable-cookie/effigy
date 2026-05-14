# 2026-05-14 16:40:45 - State Domain Extraction Follow-Through

Roadmap: [`g05.020`](../roadmaps/g05/020-reusable-core-hardening-suite.md)  
Batch card: [`745`](../roadmaps/g05/batch-cards/745-finish-state-domain-thin-shell-follow-through.md)  
Strict lane: [`083`](../specs/083-reusable-core-hardening-strict-lane.md)

## What Changed

- split `effigy-state` into focused modules:
  `model.rs`, `lineage.rs`, `paths.rs`, `history.rs`, `apply.rs`,
  `capture.rs`, `validation.rs`, and `tests.rs`
- kept `lib.rs` as a small public re-export surface
- moved pure report-path helpers and pure state context-file builders into
  `effigy-state`
- rewired `state_command.rs` to use those shared state-domain helpers

## Measured Result

- `crates/effigy-state/src/lib.rs` is now 40 lines
- `src/runner/state_command.rs` is 2150 lines, down from the 2237-line audit
  baseline
- `effigy scan god-files --json` no longer reports `effigy-state/src/lib.rs` as
  a god file

## Remaining Debt

- `state_command.rs` still owns command dispatch, side effects, task execution,
  artifact staging, SQL import, hook execution, and rendering
- that remaining size is a later extraction target if the next lane needs more
  state-shell reduction

## Validation

- `cargo test -p effigy-state`
- `cargo test state_command`
- `effigy scan god-files --json`
- `git diff --check`
