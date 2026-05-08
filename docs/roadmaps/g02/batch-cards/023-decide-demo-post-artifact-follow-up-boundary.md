# 023 Decide Demo Post-Artifact Follow-Up Boundary

Status: archived
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded browser follow-up now that Effigy has shipped
list/detail browsing, in-browser lifecycle actions, and artifact-opening
affordances.

## In Scope

- decide whether live log visibility is now the next honest browser slice
- confirm whether any tighter follow-up is needed before log visibility
- keep the decision grounded in the shipped self-hosted demos and browser
  behavior rather than abstract browser ambition

## Out Of Scope

- implementing the next browser slice
- broadening generic runtime cancellation
- embedded terminal emulation
- desktop-client decisions
- multi-attempt history or queueing

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
- the batch uses log visibility as a backdoor for terminal emulation scope

## Decision

Live log visibility is now the next honest browser slice.

Why:

- the shipped browser already covers list/detail navigation, lifecycle actions,
  and artifact opening
- the self-hosted demos exposed the next real operator gap as "what is the demo
  doing right now?" rather than another navigation affordance
- a bounded recent-output view stays inside browser-facing proof inspection
  without widening into terminal emulation or generic runtime cancellation

Still deferred:

- terminal emulation
- generic task/runtime cancellation expansion
- multi-attempt history and queueing
- desktop-client decisions

## Next Task

Implement bounded demo-browser live log visibility through
[`024-implement-demo-browser-live-log-visibility.md`](./024-implement-demo-browser-live-log-visibility.md).
