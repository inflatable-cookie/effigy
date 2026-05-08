# 490 - Wire Manager Compose Plan Into Attached Session

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move attached stream-session compose calls through manager-owned invocation
plans.

## Scope

- wire attached stream `logs --follow --tail 100` through
  `ContainerComposeInvocationPlan`
- wire attached session closeout shutdown through manager-owned plans
- preserve Ctrl+C handling, graceful child termination, shutdown policy, and
  gateway deregistration behavior
- keep TUI process-plan behavior unchanged

## Non-Goals

- no container data/cache migration yet
- no gateway TCP alias host migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when attached stream/session closeout no longer calls
lower-level compose helpers directly.

## Closeout

Attached stream sessions now build manager-owned compose invocation plans for:

- `logs --follow --tail 100`
- attached closeout shutdown

The migration preserves Ctrl+C handling, graceful child termination, shutdown
policy handling, gateway deregistration, and TUI process-plan behavior.

## Validation

- `cargo test -p effigy --lib workspace`
- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Select data/cache or gateway/support manager migration.
