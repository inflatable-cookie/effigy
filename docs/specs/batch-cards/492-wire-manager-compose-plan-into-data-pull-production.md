# 492 - Wire Manager Compose Plan Into Data Pull Production

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move data pull-production runtime bring-up through manager-owned compose
invocation plans.

## Scope

- wire `container data pull-production` compose `up` through
  `ContainerComposeInvocationPlan`
- preserve shared-service notes, readiness wait, gateway registration, hook
  execution, and report rendering
- keep data/cache operation-plan construction unchanged

## Non-Goals

- no gateway TCP alias host migration yet
- no generated image cleanup migration yet
- no `container up` bring-up migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when data pull-production no longer calls lower-level
compose helpers directly for runtime bring-up.

## Closeout

Data pull-production now builds a manager-owned compose invocation plan for
runtime bring-up.

The migration preserves shared-service notes, readiness wait, gateway
registration, hook execution, and report rendering.

## Validation

- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Select gateway/support, generated image cleanup, or container up migration.
