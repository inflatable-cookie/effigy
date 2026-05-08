# 457 - Extract Runtime Running-State and Ensure-Running Stages

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Extract the running-state check and container auto-up behavior into named
runtime activation stages.

## Scope

- split `ensure_container_runtime_prepared` so:
  - `RuntimeActivationStage::CheckRunningState` owns the primary-service
    running check
  - `RuntimeActivationStage::EnsureRunning` owns detached `container up` when
    needed
- preserve existing `run_container(ContainerSubcommand::Up { detach: true })`
  behavior
- keep exec readiness, mount prep, gateway, alias, and lease behavior unchanged
- add focused tests for:
  - already-running path skips up
  - not-running path calls up with the plan-derived container name and repo
    override

## Non-Goals

- no compose up idempotent stage migration
- no exec-readiness migration
- no gateway/alias migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when running-state detection and ensure-running behavior
are named activation stages with existing behavior preserved.

## Closeout

- `ensure_container_runtime_prepared` now delegates running detection to
  `check_runtime_running_state_stage`.
- Detached auto-up behavior now lives in `ensure_runtime_running_stage`.
- The stage preserves existing `container up --detach` behavior and uses the
  plan-derived container name and repo override.
- Focused tests cover already-running skip behavior and stopped-runtime up
  invocation shape.

## Validation

- `cargo test -p effigy --lib container_runtime_prep`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Extract mount preparation into a named runtime-prep stage.
