# 458 - Extract Runtime Mount Preparation Stage

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Extract host bind-mount preparation into a named runtime activation stage.

## Scope

- map host bind-mount preparation to `RuntimeActivationStage::PrepareMounts`
- keep existing mount path discovery and permission behavior unchanged
- keep compose up, exec readiness, gateway, alias, and lease behavior unchanged
- add focused tests that prove the stage runs before compose up and preserves
  current mount behavior

## Non-Goals

- no compose up migration
- no mount path rewrite
- no permission policy change
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when mount preparation is a named activation stage and
existing bind-mount behavior remains stable.

## Closeout

- Runtime mount prep now flows through `prepare_runtime_mounts_stage`.
- The stage maps to `RuntimeActivationStage::PrepareMounts` while preserving
  existing bind-mount discovery and permission behavior.
- Focused tests cover the stage wrapper and existing bind-mount behavior.

## Validation

- `cargo test -p effigy --lib container_runtime_prep`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Extract compose up into a named runtime-prep stage.
