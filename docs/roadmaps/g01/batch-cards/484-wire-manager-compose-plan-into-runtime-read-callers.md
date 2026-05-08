# 484 - Wire Manager Compose Plan Into Runtime Read Callers

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move the first read-only runtime compose calls behind manager-owned invocation
plans.

## Scope

- wire `container status` compose `ps` capture through
  `ContainerComposeInvocationPlan`
- wire `container logs` follow and tail commands through the same plan path
- keep current report rendering and error text stable
- preserve existing operation-plan construction from `effigy-container-ops`
- add or update focused tests if existing coverage needs adjustment

## Non-Goals

- no lifecycle/down/reset migration yet
- no data/cache migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when read-only runtime callers no longer call
`compose_args(...)` directly for status/logs execution.

## Closeout

Runtime read callers now build manager-owned compose invocation plans for:

- `container status` compose `ps`
- `container logs --follow`
- `container logs` tail capture

The existing report rendering, JSON behavior, and Colima repair retry behavior
are preserved.

## Validation

- `cargo test -p effigy-container-manager`
- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Wire the compose invocation plan into lifecycle down/reset callers.
