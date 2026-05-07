# 462 - Extract Runtime Gateway Readiness Stage

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Extract gateway readiness into a named runtime activation stage.

## Scope

- map gateway readiness to `RuntimeActivationStage::GatewayReadiness`
- keep existing gateway surface detection, gateway startup, and route
  registration behavior unchanged
- keep lease refresh behavior unchanged
- add focused tests for:
  - no gateway surface skips side effects
  - gateway surface invokes startup and registration in order

## Non-Goals

- no gateway implementation rewrite
- no lease refresh migration
- no manager/backend migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when gateway readiness is a named activation stage and
existing gateway behavior remains stable.

## Closeout

- Gateway readiness now flows through
  `ensure_runtime_gateway_readiness_stage`.
- The stage preserves gateway surface detection, gateway startup command
  rendering, and route registration.
- Focused tests cover skip behavior and startup-before-registration ordering.

## Validation

- `cargo test -p effigy --lib container_runtime_prep`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Extract lease refresh into a named activation stage.
