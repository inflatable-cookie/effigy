# 580 - Centralize Runtime Activation Plan Builder

Lane: [`055-runtime-activation-route-and-plan-authority-strict-lane.md`](../055-runtime-activation-route-and-plan-authority-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Remove duplicated activation-plan construction from runner callers.

## Scope

- add one runner-side activation plan builder behind `container_runtime_prep`
- keep `effigy-runtime-plan` dependency-light
- migrate exec, standard task, managed task, deferral, and DB seed helpers to
  the shared builder
- preserve route, repo override, policy name, container name, and lease policy
- keep side effects unchanged

## Non-Goals

- no manager/backend migration
- no public behavior changes
- no route expansion beyond current callers
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the duplicated `RuntimeActivationRequest::new(...)`
chains in runner activation helpers are gone or reduced to thin calls to the
shared builder.

## Validation

- `cargo test -p effigy-runtime-plan`
- targeted runner activation tests
- `bash scripts/check-runtime-container-drift.sh`
- `git diff --check`

## Next Task

Start
[`581-close-runtime-activation-route-authority.md`](./581-close-runtime-activation-route-authority.md).
