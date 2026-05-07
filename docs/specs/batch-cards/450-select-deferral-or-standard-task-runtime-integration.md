# 450 - Select Deferral or Standard Task Runtime Integration

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose whether the next runtime activation integration should be deferral or
standard task activation.

## Scope

- compare the remaining simple deferral activation call with standard task
  routed activation
- decide whether to complete simple callers first or start the larger standard
  task path
- create the next bounded implementation card
- keep side effects unchanged

## Non-Goals

- no side-effect migration
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the next runtime activation integration card is
ready and the lane/front-door docs point to it.

## Decision

Use deferral activation next.

Why:

- it has one activation call in the deferred container execution path
- it already has a resolved effective policy and working directory
- it uses the current runtime session context, so lease-policy mapping matches
  `effigy exec`
- it completes the simple activation callers before standard task activation

Do not start standard task activation yet. That is the main next structural
target, but it should follow after exec, DB seed, and deferral all produce
runtime activation plans.

## Closeout

Selected deferral activation and created card `451`.

## Validation

- docs path check for updated spec and roadmap front doors
- `git diff --check`

## Next Task

Start card
[`451-wire-runtime-activation-plan-into-deferral.md`](./451-wire-runtime-activation-plan-into-deferral.md).
