# 023 Decide Demo Post-Artifact Follow-Up Boundary

Status: ready
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

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

## Next Task

Use the shipped browser and self-hosted demos to decide whether live log
visibility is the next honest browser follow-up.
