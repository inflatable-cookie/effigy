# 291 Plan Dev Front Door Foundation Batch

Status: archived
Updated: 2026-04-18
Roadmap: `g02.013`
Spec: `docs/specs/013-dev-front-door-and-managed-lifecycle-strict-lane.md`

## Objective

Choose the first bounded `g02.013` execution batch now that the container
integration spine is complete.

## Scope

- assess the shipped managed-process, container, exec, and gateway substrate
- decide the smallest trustworthy first product batch for `g02.013`
- record what should stay out of that first batch
- refresh the front-door planning surfaces so `continue` resolves to one
  explicit execution card

## Out Of Scope

- implementing the dev-front-door batch itself
- embedded shell-tab work
- gateway auto-start integration
- real-project proof work
- broad redesign of the managed runtime

## Acceptance

- one explicit first execution card exists for `g02.013`
- the first batch is bounded on already-shipped substrate rather than roadmap
  ambition
- the front-door planning surfaces stop leaving `g02.013` as a broad planned
  idea

## Decision

The first `g02.013` batch should start with lifecycle ownership, not the full
daily-driver loop.

What is already real:

- managed concurrent tasks already exist as a general runtime surface
- repo-owned tasks can already target named container sessions
- attached container sessions already own startup, shutdown, and health waiting
- gateway and shared-status surfaces already exist on their own bounded paths

What is still missing in the product:

- no `tasks.<name>.managed` metadata for dev-front-door behavior
- no explicit managed-process role for `container lifecycle` versus `shell`
- no honest path where one repo-owned managed task starts a named container
  environment and tears it down on owner exit

The smallest trustworthy first batch is therefore the lifecycle foundation:

- add manifest/schema/planning support for `tasks.<name>.managed`
- add explicit concurrent-entry role shaping for dev-front-door ownership
- make one repo-owned managed task able to own container startup and shutdown
  inside the concurrent runtime
- keep shell embedding, ready-message UX, and gateway auto-start out of the
  first batch until the lifecycle path is real

## Result

The first explicit `g02.013` execution batch is now card `292`.

## Next Task

Execute `292` to land the managed dev-task and lifecycle foundation.
