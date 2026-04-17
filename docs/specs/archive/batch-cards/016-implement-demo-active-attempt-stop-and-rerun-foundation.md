# 016 Implement Demo Active-Attempt, Stop, And Rerun Foundation

Status: complete
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Implement the first honest lifecycle-control slice for Effigy's demo runner.

## In Scope

- add runner-owned active-attempt state for demos that are still executing
- add `effigy demo rerun <id>` as a fresh-attempt command
- add `effigy demo stop <id>` for demos whose active attempt is directly
  stoppable by the runner
- surface active-attempt state in `effigy demo inspect`
- make the operator-facing error path explicit when a demo is runnable but not
  stoppable through the current runtime model

## Out Of Scope

- generic cancellation support for every task-backed demo entrypoint
- multiple concurrent active attempts per demo
- TUI/browser implementation
- broad consumer-repo migration work
- implicit stop-and-rerun chaining in one command

## Acceptance Criteria

- the runner has one explicit active-attempt state model instead of inferring
  lifecycle from receipts alone
- operators can rerun a demo through a first-class command without mutating the
  previous terminal receipt
- operators can stop a directly runner-owned active demo attempt through a
  first-class command
- `demo inspect` can distinguish `running now` from `last terminal receipt`
- the implementation does not pretend generic task-backed demos are stoppable
  when Effigy does not yet own a cancellable handle for them

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Stop Conditions

- the batch starts designing browser/TUI interactions
- the batch adds multi-attempt concurrency per demo
- the batch tries to smuggle in generic task-cancellation promises without a
  real runtime handle model

## Next Task

Use the next bounded runner/planning card to decide whether the follow-up
should prioritize browser-facing state polish or broader stoppability/runtime
expansion, now that the first honest lifecycle-control slice is shipped.
