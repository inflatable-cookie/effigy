# 446 - Select First Runtime Plan Runner Integration

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the first runner integration point for `effigy-runtime-plan`.

## Scope

- compare activation callers:
  - standard task activation
  - `effigy exec`
  - DB seed
  - deferral
  - workspace seeded sessions
- select the lowest-risk call path for plan construction
- create the next bounded implementation card
- keep side effects unchanged

## Non-Goals

- no side-effect migration
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the first runner integration card is ready and the
lane/front-door docs point to it.

## Decision

Use `effigy exec` as the first runner integration point.

Why:

- it has one clear activation surface: `activate_exec_surface`
- it already passes repo root, policy, resolved container name, repo override,
  and session context in one place
- it has focused activation tests and emits host lease notices
- it avoids standard-task routing, inline workspace, auto-up retry, and
  stay-in-shell behavior

Do not start with standard task activation. That path is more important, but it
also mixes task routing, running-state checks, auto activation, workspace-seeded
session handoff, inline cleanup, and container exec. It should consume the plan
after the simple exec path proves the conversion.

## Closeout

Selected `effigy exec` activation as the first integration point and created
card `447`.

## Validation

- docs path check for updated spec and roadmap front doors
- `git diff --check`

## Next Task

Start card
[`447-wire-runtime-activation-plan-into-exec-surface.md`](./447-wire-runtime-activation-plan-into-exec-surface.md).
