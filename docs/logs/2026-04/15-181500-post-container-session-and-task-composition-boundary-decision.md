# Post Container Session And Task Composition Boundary Decision

Date: 2026-04-15
Roadmap: `g02.006`
Spec: `docs/specs/006-colima-container-environment-strict-lane.md`
Batch Card: `docs/roadmaps/g02/batch-cards/109-decide-post-container-session-and-task-composition-boundary.md`

## Decision

Pause `g02.006` on the current v1 container boundary.

## Why

The lane now has enough real evidence to stop widening:

- `effigy container` is a real command surface with named/default container
  resolution, Colima startup, compose lifecycle, host port/mount policy, and
  health gating
- attached sessions are no longer only raw log-follow; they now have an
  Effigy-owned session shape
- repos can now expose named container sessions through
  `container_session = "..."` without dropping back to raw compose shell glue
- `example-site` proved detached bring-up, running status, graceful teardown,
  and repo-owned task-session launch on a real machine

The remaining open question is narrower and explicit:

- non-interactive live-stop behavior on the real `colima nerdctl compose` path
  is still less trustworthy than the targeted runtime tests

That limit does not break the v1 claim Effigy now makes. It is an operator
hardening edge, not a hidden contract hole in named containers, attached
sessions, or repo-owned task composition.

## Boundary

What is now trustworthy to claim:

- first-class `effigy container ...` support for one named Colima-backed local
  environment contract
- optional default container resolution
- explicit host-facing ports and repo-relative mounts
- explicit `primary_service`
- attached Effigy-owned container sessions
- repo-owned task aliases via `container_session = "..."`
- one honest bounded consumer proof in `example-site`

What remains explicitly deferred:

- hardening the real-machine non-interactive live-stop path beyond the
  targeted runtime tests
- deeper convergence between container sessions and existing repo multi-process
  `dev` stacks
- broader driver abstraction or host-service convenience policy

## Outcome

`g02.006` is paused. Reopening it now would be churn unless a real operator
need makes the deferred live-stop edge concrete enough to justify another
bounded hardening batch.

The next product move is not more container widening. It is release closure on
`g02.007` for the shipped optional distribution surface.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved: the container lane shifted from active implementation into a paused
  v1 boundary with shipped attached sessions, repo-owned task composition, and
  one explicit deferred real-operator edge
- Remaining open: real-machine live-stop hardening on the `colima nerdctl`
  path, if a future operator proof shows the test-backed boundary is not
  enough

## Next Task

Activate `g02.007` next so the shipped optional distribution surface can close
through an Effigy release and consumer rollout plan.
