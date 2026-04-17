# 006 Colima Container Environment Strict Lane

Status: paused
Updated: 2026-04-15
Roadmap: `g02.006`

## Context

The optional distribution lane is paused on a trustworthy boundary. The next
highest-priority product problem is the user's blocked local web-development
workflow on a new machine.

Effigy needs a first-class container-environment surface so web-oriented repos
can stop depending on host-installed databases, PHP runtimes, reverse proxies,
and related service sprawl.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/006-colima-container-environment-contract.md`

## Lane Focus

The active strict lane is:

- define the v1 `effigy container` command surface
- define the manifest registry of named container environments
- define the attached owner-session lifecycle
- define the task integration boundary without overloading `effigy dev`
- settle the host/service integration boundary enough that execution does not
  invent it ad hoc

## Current Posture

`strict-paused`

The first implementation surface is now real:

- `effigy container ...` is the primary surface
- container environments live under a named manifest registry
- default container resolution is explicit
- host-facing ports and repo-relative mounts are manifest-owned policy
- `primary_service` is the explicit interactive shell target
- attached sessions shut down on owner exit by default
- Colima startup now works on hosts that only have Colima installed by falling
  back to `colima nerdctl` plus `--runtime containerd`
- `contact-patch` now acts as the first honest consumer proof for detached
  bring-up, running status, graceful teardown, and repo-owned task-session
  launch on a real web-oriented repo
- attached sessions now widen into a real Effigy session shape instead of only
  raw log follow
- repos can now expose named container sessions through `container_session =
  "..."` task aliases without dropping back to raw compose shell glue

That hardening edge is now closed:

- the real-machine live-stop and closeout path is now proven strongly enough in
  `contact-patch`
- the current v1 container boundary is trustworthy enough to pause
- the next valid move is release closure on `g02.007`, not more container
  churn

## Batch Model

- planning stays in this spec plus the roadmap
- execution proceeds only from a ready card
- each ready card must leave the lane either:
  - with another explicit ready card
  - or back in planning with an intent checkpoint

## Intent Checkpoint

If the container question broadens, stop and ask whether the priority is:

- command and manifest contract
- session/TUI ownership model
- or host/service integration policy

Do not guess.

## Exit Condition

This strict lane is complete when Effigy has a trustworthy v1 container
environment command surface, one honest consumer proof, and one explicit
decision about whether the missing operator UX/task-composition work is small
enough to pause or still needs one more bounded batch.

## Next Task

Keep `g02.006` paused on the current container boundary and activate
`g02.007` for the Effigy distribution release closure work.
