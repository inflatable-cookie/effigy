# 017 Decide Demo Post-Lifecycle Follow-Up Boundary

Status: archived
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after the shipped demo lifecycle-control
foundation.

## In Scope

- decide whether the next batch should prioritize browser-facing state polish
  or broader stoppability/runtime expansion
- define the minimum runner/query surface the chosen follow-up actually needs
- keep the generic task-cancellation boundary explicit if it is still not
  runtime-owned

## Out Of Scope

- implementing generic task cancellation
- starting TUI/browser implementation
- broad consumer-repo migration work
- widening into attempt history or multi-attempt concurrency

## Acceptance Criteria

- the roadmap states one explicit next slice instead of two blurred themes
- the next slice has a bounded objective and clear runtime boundary
- the lane does not over-claim generic stoppability if the runtime still lacks
  honest cancellable handles

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch starts implementing browser UI
- the batch starts promising generic task-backed cancellation
- the batch starts designing multi-attempt history or queueing

## Next Task

Open the next bounded execution card for browser-facing state/query polish and
keep broader stoppability deferred until the runtime can expose cancellable
handles honestly.
