# 025 Decide Demo Post-Live-Log Follow-Up Boundary

Status: ready
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded browser follow-up now that Effigy ships list/detail
navigation, lifecycle actions, artifact-opening, and bounded live log
visibility inside `effigy demo browser`.

## In Scope

- decide whether the next browser slice should prioritize richer log handling,
  artifact/detail polish, or another tighter operator-visible gap
- keep the choice grounded in the shipped self-hosted demos and browser
  behavior rather than abstract UI ambition
- keep deferred runtime and desktop concerns explicitly out of the follow-up

## Out Of Scope

- implementing the next browser slice
- terminal emulation
- generic runtime cancellation expansion
- multi-attempt history browsing
- desktop-client work

## Acceptance Criteria

- the next browser priority is explicit
- the deferred concerns stay clearly deferred instead of drifting back in
- the lane remains bounded around operator-visible proof browsing

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch turns into implementation without a settled next-slice decision
- the batch reopens settled demo model, lifecycle, or browser contracts
- the batch uses recent-output visibility as a backdoor for terminal emulation
  scope

## Next Task

Choose the next bounded browser follow-up after live log visibility, then open
one explicit ready card for that narrower slice.
