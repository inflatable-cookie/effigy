# 485 - Wire Manager Compose Plan Into Lifecycle Down Reset

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move lifecycle shutdown/reset compose calls behind manager-owned invocation
plans.

## Scope

- wire runtime `container down` compose shutdown through
  `ContainerComposeInvocationPlan`
- wire runtime `container reset` keep-data and wipe-data shutdown commands
  through the same plan path
- preserve current cleanup behavior and error text where practical
- keep existing operation-plan construction from `effigy-container-ops`

## Non-Goals

- no `container up` attached-session migration yet
- no exec/shell/data/cache migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when lifecycle down/reset runtime callers no longer call
`compose_args(...)` directly for shutdown execution.

## Closeout

Lifecycle shutdown/reset callers now build manager-owned compose invocation
plans for:

- `container down`
- `container down --all`
- scoped `container down`
- `container reset --keep-data`
- `container reset --wipe-data`

Shutdown behavior remains unchanged, including immediate shutdown's ordered
`kill` then `down --remove-orphans` sequence and Colima repair retry behavior.

## Validation

- `cargo test -p effigy-container-manager`
- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Select the next exec/shell or data/cache manager migration slice.
