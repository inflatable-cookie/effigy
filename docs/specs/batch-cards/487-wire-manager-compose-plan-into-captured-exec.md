# 487 - Wire Manager Compose Plan Into Captured Exec

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move captured container exec through manager-owned compose invocation plans.

## Scope

- wire `run_container_exec_capture_with_options` through
  `ContainerComposeInvocationPlan`
- preserve stdin-file handling, working-dir resolution, color env behavior, and
  handoff env injection
- keep existing exec operation-plan construction from `effigy-container-ops`
- preserve current Rhai/container exec behavior

## Non-Goals

- no interactive shell/session migration yet
- no data/cache migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when captured container exec no longer calls lower-level
compose exec helpers directly.

## Closeout

Captured container exec now builds a manager-owned compose invocation plan
before execution.

The migration preserves:

- stdin-file handling
- primary-service working-dir resolution
- color env behavior
- container handoff env injection
- Colima direct exec behavior behind the execution adapter
- Rhai container exec behavior

## Validation

- `cargo test -p effigy --lib container_command`
- `cargo test -p effigy-rhai`
- `git diff --check`

## Next Task

Wire manager compose plans into interactive shell/session callers.
