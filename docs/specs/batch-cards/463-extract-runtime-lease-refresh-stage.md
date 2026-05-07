# 463 - Extract Runtime Lease Refresh Stage

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Extract host-container lease refresh into a named runtime activation stage.

## Scope

- map lease refresh to `RuntimeActivationStage::LeaseRefresh`
- make the internal activation executor call a named lease-refresh stage
- preserve skip-refresh behavior
- preserve existing refreshed-lease output semantics
- add focused tests for refresh and skip policy behavior

## Non-Goals

- no lease implementation rewrite
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when lease refresh is a named activation stage and
existing activation reports remain stable.

## Closeout

- Host-container lease refresh now flows through
  `refresh_runtime_lease_stage`.
- The stage preserves refresh vs skip policy behavior and refreshed-lease
  output semantics.
- Focused tests cover both refresh and skip paths.

## Validation

- `cargo test -p effigy --lib container_runtime_prep`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Close or widen the runtime activation stage extraction slice.
