# 456 - Extract Runtime Policy Validation Stage

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Extract runtime policy and backend validation into the first named activation
stage behind the runtime activation pipeline.

## Scope

- split policy/backend validation out of `validate_policy_runtime`
  orchestration where useful
- add a small stage function or module that maps directly to
  `RuntimeActivationStage::ValidatePolicy` and
  `RuntimeActivationStage::ValidateBackend`
- preserve existing error messages and `RunnerError::ContainerRuntimePolicy`
  phases
- keep compose/backend command execution unchanged
- add focused tests for stage ordering and error phase stability

## Non-Goals

- no compose up migration
- no exec-readiness migration
- no gateway/alias migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when runtime policy/backend validation is a named
activation stage and existing validation behavior remains stable.

## Closeout

- `validate_policy_runtime` now delegates to named activation validation
  stages.
- `RuntimeActivationStage::ValidatePolicy` and
  `RuntimeActivationStage::ValidateBackend` map to focused stage functions.
- Existing `RunnerError::ContainerRuntimePolicy` phases remain stable:
  `policy validation` and `backend validation`.
- Focused tests cover policy-stage error phase stability.

## Validation

- `cargo test -p effigy --lib container_runtime_prep`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Extract running-state and ensure-running activation stages.
