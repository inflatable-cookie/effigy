# 461 - Extract Runtime Alias Reconciliation Stage

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Extract primary-service TCP alias reconciliation into a named runtime
activation stage.

## Scope

- map alias reconciliation to `RuntimeActivationStage::AliasReconciliation`
- keep existing alias reconciliation behavior and error propagation unchanged
- keep gateway and lease behavior unchanged
- add focused tests for stage wrapper behavior and runtime-prep ordering

## Non-Goals

- no gateway stage migration
- no manager/backend migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when alias reconciliation is a named activation stage and
existing runtime-prep ordering remains stable.

## Closeout

- Primary-service TCP alias reconciliation now flows through
  `reconcile_runtime_aliases_stage`.
- Existing ordering remains mount prep, compose up, exec readiness, alias
  reconciliation.
- Focused tests cover stage identity and error propagation without live
  containers.

## Validation

- `cargo test -p effigy --lib container_runtime_prep`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Extract gateway readiness into a named activation stage.
