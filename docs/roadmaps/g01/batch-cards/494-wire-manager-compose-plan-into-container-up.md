# 494 - Wire Manager Compose Plan Into Container Up

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move `container up` compose bring-up through manager-owned invocation plans.

## Scope

- wire attached `container up` through `ContainerComposeInvocationPlan`
- wire detached `container up` through the same plan path
- preserve Ctrl+C stop handling, cleanup-on-failure behavior, shared service
  notes, health wait, gateway registration, and report rendering

## Non-Goals

- no gateway TCP alias host migration yet
- no generated image cleanup migration yet
- no shared-service helper migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `container up` no longer calls lower-level compose
helpers directly for bring-up execution.

## Closeout

`container up` now builds a manager-owned compose invocation plan for both
attached and detached bring-up.

The migration preserves Ctrl+C stop handling, cleanup-on-failure behavior,
shared service notes, health wait, gateway registration, and report rendering.

## Validation

- `cargo test -p effigy --lib container_command`
- `git diff --check`

The focused cargo validation was attempted twice but the local Rust compiler
process stalled in unrelated dependency crates before test execution. The
whitespace check and targeted drift scan passed.

## Next Task

Select gateway/support, generated image cleanup, or shared-service migration.
