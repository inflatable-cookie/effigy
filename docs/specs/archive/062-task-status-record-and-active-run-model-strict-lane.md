# 062 - Task Status Record And Active Run Model Strict Lane

Roadmap: [`g04.020`](../roadmaps/g04/020-task-status-record-and-active-run-model.md)

Status: Complete
Owner: Platform
Created: 2026-05-10
Completed: 2026-05-10

## Purpose

Define one canonical task-status record model and one shared write-side path so
later operator surfaces can answer task status without degrading into lock
inspection or caller-local runtime guesses.

## Hard Boundaries

- keep task status task-scoped, not machine-global
- do not widen into final `effigy tasks status` UX in this lane
- do not add stop, restart, tail, or repair verbs
- do not make non-task built-ins first-class task-status targets
- keep write ownership in the shared execution pipeline, not caller-local
  branches
- no retention or pruning policy yet
- no `.github/workflows/` edits
- no release execution

## Execution Chain

- `617` complete: opened the lane, promoted the first task-status contract
  anchor, and selected the first contract-shaping card
- `618` complete: locked task-status identity, state/stage taxonomy, active/completed
  persistence layout, and stale-record reconciliation boundary before
  implementation
- `619` complete: added typed task-status key/record types and runtime/report
  path helpers
- `620` complete: wired the shared task-status writer into the canonical direct
  execution path and proved succeeded, failed, cancelled, and blocked outcomes
- `621` complete: added typed active/latest read helpers plus active-record
  liveness and stale-reconciliation results for the later read/query lane

## Exit Condition

This lane is complete when Effigy has one canonical task-status record model,
one shared write-side status owner across task execution surfaces, and enough
proof coverage that the later read/query lane can trust the persisted status
truth.

## Next Task

Open the next strict lane under `g04.021` and build the read/query surface on
top of this completed task-status record model.
