# 295 Decide Post Shell Role Follow-Up

Status: landed
Updated: 2026-04-18
Roadmap: `g02.013`
Spec: `docs/specs/013-dev-front-door-and-managed-lifecycle-strict-lane.md`

## Objective

Choose the next bounded `g02.013` batch now that managed dev tasks can own
both container lifecycle and an embedded container shell.

## Scope

- assess the remaining `g02.013` gaps against the shipped lifecycle and shell
  foundation
- decide whether readiness UX or gateway auto-start is the next bounded batch
- refresh the front-door planning surfaces so `continue` resolves to one
  explicit execution card

## Out Of Scope

- implementing the follow-up batch itself
- broad `effigy dev` closeout or real-project proof work
- widening multiple `g02.013` concerns at once

## Acceptance

- one explicit next execution card exists for `g02.013`
- the chosen batch is bounded on already-shipped substrate
- the lane front doors stop pointing at `294`

## Decision

The next `g02.013` batch should be readiness UX, not gateway auto-start.

Why readiness comes first:

- `294` already gives the managed dev task the second owned tab the daily
  driver needed, so the next missing piece is honest "when is this usable?"
  feedback inside the same runtime
- the lifecycle owner already starts the named `container_session` through the
  shipped detached container path, so health waiting and ready-state projection
  are the next narrow follow-through on the same product seam
- the roadmap already calls for `tasks.<name>.managed.health_wait` and
  `ready_message`, and those fields stay within the repo-owned task contract
  instead of widening into another subsystem
- gateway auto-start still depends on the gateway lane's DNS-owned surfaces and
  would widen the batch across a second integration seam before the managed dev
  loop can even report local readiness honestly

What stays out of the next batch:

- `tasks.<name>.managed.gateway`
- broader multi-service readiness orchestration
- real-project proof and lane closeout

## Result

The next explicit `g02.013` execution batch is now card `296`.

## Next Task

Execute `296` to land the managed dev readiness UX foundation.
