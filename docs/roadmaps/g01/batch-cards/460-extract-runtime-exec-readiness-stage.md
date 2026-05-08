# 460 - Extract Runtime Exec Readiness Stage

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Extract primary-service exec readiness into a named runtime activation stage.

## Scope

- map exec readiness to `RuntimeActivationStage::ExecReadiness`
- keep existing short probe, restart, longer probe behavior unchanged
- keep dependent service restart behavior unchanged
- keep alias reconciliation, gateway, and lease behavior unchanged
- add focused tests for stage wrapper behavior and error stability

## Non-Goals

- no alias/gateway migration
- no manager/backend migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when exec readiness is a named activation stage and
existing recovery behavior remains stable.

## Closeout

- Primary-service exec readiness now flows through
  `ensure_runtime_exec_readiness_stage`.
- The stage preserves existing probe, restart, and recovery behavior.
- Focused tests use an injectable stage helper so readiness behavior is covered
  without live containers.

## Validation

- `cargo test -p effigy --lib container_runtime_prep`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Extract alias reconciliation runtime-prep stage.
