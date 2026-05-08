# 021 Decide Demo Browser Follow-Up Slice

Status: archived
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded browser slice now that Effigy has a shipped list/detail
demo browser with in-browser action dispatch.

## In Scope

- decide whether the next browser batch should prioritize live log visibility
  or artifact-opening affordances
- define the minimum operator need that the follow-up slice must satisfy
- keep the decision grounded in the shipped `browser-proof-report` and
  `lifecycle-window` demos rather than abstract UI ambition
- leave a single honest ready card for the next implementation or return the
  lane to planning if the boundary is not yet coherent

## Out Of Scope

- implementing the next browser slice
- broadening generic runtime cancellation
- embedded terminal emulation
- desktop-client decisions
- multi-attempt history or queueing

## Acceptance Criteria

- the next browser slice has one explicit priority
- the deferred browser concern is recorded clearly instead of drifting
- the lane does not blur browser follow-through with broader runtime expansion

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch turns into implementation without a clear next-slice decision
- the batch reopens settled demo model, lifecycle, or browser-foundation
  contracts
- the batch uses desktop-client pressure to justify skipping bounded browser
  sequencing

## Next Task

Implement the next bounded browser slice around artifact-opening affordances,
then revisit live log visibility separately.
