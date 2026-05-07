# 293 Decide Post Lifecycle Foundation Follow-Up

Status: landed
Updated: 2026-04-18
Roadmap: `g02.013`
Spec: `docs/specs/013-dev-front-door-and-managed-lifecycle-strict-lane.md`

## Objective

Choose the next bounded `g02.013` batch now that managed dev-task lifecycle
ownership is real on the product path.

## Scope

- assess the remaining `g02.013` gaps against the shipped lifecycle foundation
- decide whether shell embedding, readiness UX, or gateway automation is the
  next highest-value bounded batch
- refresh the front-door planning surfaces so `continue` resolves to one
  explicit execution card

## Out Of Scope

- implementing the follow-up batch itself
- broad `effigy dev` closeout or real-project proof work
- widening multiple `g02.013` concerns at once

## Acceptance

- one explicit next execution card exists for `g02.013`
- the chosen batch is bounded on already-shipped substrate
- the lane front doors stop pointing at `292`

## Decision

The next `g02.013` batch should be the embedded shell role, not readiness UX
or gateway auto-start.

Why shell comes first:

- `292` already made one repo-owned managed task able to own container startup
  and shutdown honestly
- the container lane already ships `effigy container shell` against the primary
  service, so the substrate for a bounded shell role already exists
- the roadmap's daily-driver gap after lifecycle ownership is still "where does
  the developer work?" more than "what status text do they see?"
- readiness waiting already happens through the bounded `container up --detach`
  path used by the lifecycle process, so the remaining health work is mostly UI
  projection instead of missing core ownership
- gateway auto-start is real product value, but it widens the batch into
  another subsystem seam before the TUI's second owned dev tab exists

What stays out of the next batch:

- `tasks.<name>.managed.gateway`
- `tasks.<name>.managed.health_wait`
- `tasks.<name>.managed.ready_message`
- broader real-project proof and closeout

## Result

The next explicit `g02.013` execution batch is now card `294`.

## Next Task

Execute `294` to land the embedded shell-role foundation for managed dev
tasks.
