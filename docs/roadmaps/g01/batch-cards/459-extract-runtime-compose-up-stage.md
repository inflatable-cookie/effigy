# 459 - Extract Runtime Compose Up Stage

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Extract the idempotent compose up call into a named runtime activation stage.

## Scope

- map idempotent compose up to `RuntimeActivationStage::ComposeUp`
- preserve current best-effort behavior where compose up failure is ignored and
  exec readiness decides whether runtime prep fails
- keep exec readiness, alias reconciliation, gateway, and lease behavior
  unchanged
- add focused tests for compose up argument shape and best-effort behavior

## Non-Goals

- no backend/manager migration
- no exec-readiness migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when compose up is a named activation stage and existing
best-effort behavior remains stable.

## Closeout

- Idempotent compose up now flows through `run_runtime_compose_up_stage`.
- The stage preserves the existing best-effort behavior: compose up errors are
  ignored and exec readiness remains the failure authority.
- Focused tests cover full compose argument shape and best-effort error
  behavior.

## Validation

- `cargo test -p effigy --lib container_runtime_prep`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Extract exec-readiness runtime-prep stage.
