# 454 - Select Runtime Prep Stage Migration Slice

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the first side-effect stage to move behind the runtime activation
pipeline after the main runner activation surfaces have plan coverage.

## Scope

- inspect `src/runner/container_runtime_prep/mod.rs`
- choose one bounded stage:
  - policy/load validation
  - backend/runtime running check
  - compose up
  - exec readiness
  - gateway/alias reconciliation
  - lease refresh
- create the next implementation card with exact files, validation, and
  non-goals
- do not move code in this selection card

## Non-Goals

- no code migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the next runtime-prep stage migration card is ready
and scoped tightly enough to implement without re-planning.

## Closeout

Selected first side-effect migration:

- move the activation executor in `src/runner/container_runtime_prep/mod.rs`
  behind `RuntimeActivationPlan`
- keep the existing stage functions and side effects intact
- make the activation report derivable from the plan and existing activation
  result

This is the smallest useful bridge from plan-only coverage to real stage
execution because all current runner integrations still converge through
`activate_container_runtime_for_task`.

## Validation

- docs path/link check by inspection
- `git diff --check`

## Next Task

Move the runtime-prep activation executor behind `RuntimeActivationPlan`.
