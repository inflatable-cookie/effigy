# 621 - Add Task Status Active Record Liveness And Stale Reconciliation Helpers

Lane: [`062-task-status-record-and-active-run-model-strict-lane.md`](../062-task-status-record-and-active-run-model-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-10
Completed: 2026-05-10

## Goal

Teach the task-status layer how to tell live active records from stale ones so
later read/query surfaces can trust `running` only when the runtime evidence is
still real.

## Scope

- add typed read helpers for active and latest completed task-status records
- add first-round active-record liveness checks for pid and heartbeat freshness
- classify stale active records without deleting them
- return stale-record warnings/evidence alongside reconciliation results
- keep the helpers task-scoped and repo-local

## Non-Goals

- no final `effigy tasks status` CLI surface yet
- no stale-record cleanup command
- no machine-wide inventory
- no retention or pruning policy
- no `.github/workflows/` edits
- no release work

## Exit Condition

This card is complete when the task-status layer can load active and completed
records by key, reconcile live versus stale active state, and surface enough
evidence that the later read/query lane can trust the result without caller-
local lock inspection.

## Validation

- focused liveness helper tests
- stale active record fallback tests
- pid-live and stale-heartbeat tests
- `cargo fmt --all -- --check`
- `git diff --check`

## Closeout

The runtime task-status layer can now:

- load active records by key
- load latest completed records by key
- reconcile live active state versus stale active state
- fall back to latest completed status without deleting stale active files
- surface stale-record warning codes and messages for the later query/report
  layer

## Next Task

Open the read-side query lane behind:

- `effigy tasks status <selector>`
- `effigy tasks status --all`
