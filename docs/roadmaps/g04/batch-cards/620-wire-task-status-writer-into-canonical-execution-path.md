# 620 - Wire Task Status Writer Into Canonical Execution Path

Lane: [`062-task-status-record-and-active-run-model-strict-lane.md`](../062-task-status-record-and-active-run-model-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Goal

Attach the shared task-status writer to the canonical execution path so direct
task execution starts producing active and completed status records without
caller-local special cases.

## Scope

- add one shared task-status writer owner on the canonical execution path
- write active records before task execution begins
- write completed records on success, failure, blocked, and cancelled outcomes
- remove or supersede active records when execution closes out
- cover direct routed execution plus any other surfaces that already converge
  through the same path
- keep later caller-surface widening for a follow-up card if needed

## Non-Goals

- no final `effigy tasks status` read/query surface
- no machine-wide inventory
- no stale-record cleanup command
- no explicit watch/bootstrap/managed widening if those surfaces do not already
  converge through the canonical path
- no release work

## Exit Condition

This card is complete when canonical task execution writes trustworthy active
and completed task-status records through one shared owner and the proof matrix
shows direct execution status truth without caller-local divergence.

## Validation

- focused direct-execution task-status write tests
- completed-record outcome tests for success, failed, blocked, and cancelled
- active-record closeout tests
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Widen task-status writer coverage to additional converged execution surfaces if
needed, or move to stale/live reconciliation helpers if the direct path is
already representative.
