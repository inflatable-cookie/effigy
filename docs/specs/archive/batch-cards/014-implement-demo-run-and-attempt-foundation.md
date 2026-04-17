# 014 Implement Demo Run And Attempt Foundation

Status: complete
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Implement the next bounded demo-runner slice in Effigy.

## In Scope

- add `effigy demo run <id>` in text and JSON modes
- execute both task-backed and run-backed demo entrypoints
- create normalized latest-attempt state so `effigy demo inspect` reflects new
  runs
- write baseline pass/fail outcome and receipt metadata for the executed demo

## Out Of Scope

- `effigy demo stop`
- `effigy demo rerun`
- TUI/browser implementation
- broad consumer-repo migration work

## Acceptance Criteria

- operators can execute one declared demo through `effigy demo run <id>`
- successful and failed runs both update normalized latest-attempt state
- `effigy demo inspect <id>` reflects the newly recorded attempt without
  relying on Signal-specific scripts
- the implementation keeps later stop/rerun and browser work for separate
  bounded batches

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Stop Conditions

- the batch starts adding stop/rerun lifecycle work
- the batch drifts into TUI/browser implementation
- attempt recording becomes coupled to one consumer repo's receipt layout

## Next Task

Use the next bounded `g02.003` ready card to decide active-attempt, stop, and
rerun semantics before more lifecycle control is implemented.
