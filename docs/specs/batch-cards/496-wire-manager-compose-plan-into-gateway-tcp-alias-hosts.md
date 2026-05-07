# 496 - Wire Manager Compose Plan Into Gateway TCP Alias Hosts

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move gateway TCP alias host updates through manager-owned compose invocation
plans.

## Scope

- wire primary-service TCP alias host update `compose exec` through
  `ContainerComposeInvocationPlan`
- preserve route resolution, rendered host script, note rendering, and error
  behavior
- keep gateway route registration logic unchanged

## Non-Goals

- no shared-service helper migration yet
- no generated image cleanup migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when gateway TCP alias host updates no longer call
lower-level compose helpers directly.

## Closeout

Gateway TCP alias host updates now build manager-owned compose invocation
plans before execution.

Route resolution, host script rendering, note rendering, and gateway route
registration behavior are unchanged.

## Validation

- `cargo test -p effigy --lib container_command::support`
- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Select shared-service bring-up or generated image cleanup migration.
