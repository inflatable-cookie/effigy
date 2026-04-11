# 015 Decide Demo Active-Attempt, Stop, And Rerun Contract

Status: complete
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Lock the next lifecycle contract for Effigy's demo runner before more command
surface is implemented.

## In Scope

- define what counts as an active demo attempt after `demo run` now exists
- decide whether stop/rerun target demos, attempts, or both
- decide the minimum persisted state needed for `demo stop` and `demo rerun`
- decide whether lifecycle control needs attempt ids, run handles, or both in
  the next execution slice

## Out Of Scope

- implementing `effigy demo stop`
- implementing `effigy demo rerun`
- TUI/browser work
- consumer-repo migration work

## Acceptance Criteria

- the roadmap states one explicit active-attempt model
- the contract makes the target shape for stop and rerun commands unambiguous
- the next execution slice is bounded enough to implement without reopening the
  lifecycle model
- the lane does not guess its way into process-control behavior that the model
  does not yet support

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch starts implementing process-control runtime behavior
- the batch drifts into TUI/browser design
- the contract depends on Signal-specific runner conventions

## Next Task

Execute the next bounded runner card for active-attempt state plus the first
honest `demo stop` and `demo rerun` slice.
