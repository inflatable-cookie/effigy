# 449 - Wire Runtime Activation Plan Into DB Seed

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make DB seed runtime prep build a `RuntimeActivationPlan` before calling the
existing activation side effects.

## Scope

- build a runtime activation request/plan in `prepare_db_seed_runtime`
- map `data_seed_runtime_session_context()` lease policy into the plan
- preserve the existing `activate_container_runtime_for_task` side-effect path
- keep DB seed source handling, artifact staging, task dispatch, and output
  unchanged
- add or adjust focused tests for plan identity and lease-policy mapping where
  existing fixtures make that cheap

## Non-Goals

- no seed source or artifact behavior changes
- no standard task activation migration
- no deferral migration
- no side-effect migration
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when DB seed activation constructs a runtime activation
plan and existing DB seed behavior remains unchanged.

## Validation

- `cargo test -p effigy --lib db_seed`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Closeout

DB seed runtime prep now builds a `RuntimeActivationPlan` before calling the
existing activation side effects. The plan carries repo root, policy name,
container name, repo override, and mapped data-seed lease policy.

DB seed source handling, artifact staging, task dispatch, and output are
unchanged.

## Next Task

Start card
[`450-select-deferral-or-standard-task-runtime-integration.md`](./450-select-deferral-or-standard-task-runtime-integration.md).
