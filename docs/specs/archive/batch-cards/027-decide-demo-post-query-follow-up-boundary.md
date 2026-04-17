# 027 Decide Demo Post-Query Follow-Up Boundary

Status: complete
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded browser follow-up now that Effigy ships list/detail
navigation, lifecycle actions, artifact-opening, bounded recent output, and
in-browser query controls.

## In Scope

- decide whether the next browser slice should prioritize richer detail/log
  polish, broader browse ergonomics, or another tighter operator-visible gap
- keep the choice grounded in the shipped self-hosted demos and browser
  behavior rather than UI ambition
- keep deferred runtime and desktop concerns explicitly out of the follow-up

## Out Of Scope

- implementing the next browser slice
- terminal emulation
- generic runtime cancellation expansion
- multi-attempt history browsing
- desktop-client work

## Acceptance Criteria

- the next browser priority is explicit
- deferred concerns stay clearly deferred instead of drifting back in
- the lane remains bounded around operator-visible proof browsing

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch turns into implementation without a settled next-slice decision
- the batch reopens settled demo model, lifecycle, or browser contracts
- the batch uses query controls as a backdoor for broader UI/runtime scope

## Next Task

Implement bounded detail-pane navigation through
[`028-implement-demo-browser-detail-navigation.md`](./028-implement-demo-browser-detail-navigation.md).
