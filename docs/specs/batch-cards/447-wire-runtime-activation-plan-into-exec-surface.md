# 447 - Wire Runtime Activation Plan Into Exec Surface

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make `effigy exec` build a `RuntimeActivationPlan` before calling the existing
activation side effects.

## Scope

- add `effigy-runtime-plan` as a runner dependency
- build a runtime activation request/plan in `activate_exec_surface`
- map runner lease refresh policy into the plan lease policy
- keep `activate_container_runtime_for_task` as the side-effect executor
- keep output and activation notice behavior unchanged
- add or adjust focused tests for plan identity and lease-policy mapping

## Non-Goals

- no side-effect migration
- no standard task activation migration
- no DB seed or deferral migration
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `effigy exec` constructs a runtime activation plan
and existing exec behavior remains unchanged.

## Validation

- `cargo test -p effigy --lib exec_command`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Closeout

`effigy exec` now builds a `RuntimeActivationPlan` before calling the existing
container activation side effects. The plan carries repo root, policy name,
resolved container name, repo override, and mapped lease policy.

Activation execution, output, and lease notice behavior are unchanged.

## Next Task

Start card
[`448-select-next-runtime-activation-integration.md`](./448-select-next-runtime-activation-integration.md).
