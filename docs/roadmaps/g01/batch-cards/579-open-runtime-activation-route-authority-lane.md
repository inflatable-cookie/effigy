# 579 - Open Runtime Activation Route Authority Lane

Lane: [`055-runtime-activation-route-and-plan-authority-strict-lane.md`](../055-runtime-activation-route-and-plan-authority-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make activation route identity explicit in the runtime activation request and
wire the first caller routes.

## Scope

- add route selection to `RuntimeActivationRequest`
- make `RuntimeActivationPlan::from_request()` use the request route
- add activation route variants needed by current runner surfaces
- set route identity for exec, standard task, managed task, deferral, and DB
  seed activation callers
- add focused tests for default and non-task route identity
- update the lane/roadmap front doors

## Non-Goals

- no shared builder extraction yet
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when non-task activation callers no longer get silently
reported as `Task`.

## Validation

- `cargo test -p effigy-runtime-plan`
- targeted runner tests for touched activation callers
- `bash scripts/check-runtime-container-drift.sh`
- `git diff --check`

## Next Task

Start
[`580-centralize-runtime-activation-plan-builder.md`](./580-centralize-runtime-activation-plan-builder.md).
