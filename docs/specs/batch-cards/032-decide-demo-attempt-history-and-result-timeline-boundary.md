# 032 Decide Demo Attempt History And Result Timeline Boundary

Status: complete
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the first bounded runner-side follow-up after the shipped browser
baseline: persisted attempt history and result timelines beyond the current
active-plus-latest model.

## In Scope

- assess the now-shipped demo runner and browser against the remaining
  operator-visible gap around "what happened before the latest attempt"
- decide whether Effigy should record a bounded attempt history per demo
- define the minimum result/history shape needed for CLI and later browser use
  without widening into generic runtime expansion

## Out Of Scope

- multi-attempt concurrent execution
- terminal emulation or richer log streaming
- more browser layout polish
- repo migration or desktop-client work

## Acceptance Criteria

- the next demo slice is explicitly runner-side rather than another browser-only
  polish pass
- the history/result boundary stays bounded and does not expand into queueing or
  generic runtime orchestration
- the resulting next task leaves the lane with one clean ready card for the
  first attempt-history implementation or a narrower planning step if needed

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch turns into implementation instead of a boundary decision
- the batch widens into generic cancellation, queueing, or desktop-client
  planning
- the batch reopens browser detail ergonomics without a runner-state reason

## Next Task

Execute [`033-implement-demo-attempt-history-foundation.md`](./033-implement-demo-attempt-history-foundation.md)
to deliver bounded persisted terminal-attempt history through the runner and
`demo inspect` before widening list or browser rendering.
