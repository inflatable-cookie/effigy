# 498 - Wire Manager Compose Plan Into Shared Service Bring Up

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move shared-service compose bring-up through manager-owned invocation plans.

## Scope

- wire shared-service `compose up -d` through `ContainerComposeInvocationPlan`
- preserve shared-service notes and error behavior
- keep shared-service policy resolution unchanged

## Non-Goals

- no generated image cleanup migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when shared-service bring-up no longer calls direct
runtime backend helpers.

## Closeout

Shared-service bring-up now builds a manager-owned compose invocation plan
before execution.

Shared-service policy resolution, notes, and error rendering are unchanged.

## Validation

- `cargo test -p effigy --lib container_command::support`
- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Migrate generated image cleanup or close remaining drift.
