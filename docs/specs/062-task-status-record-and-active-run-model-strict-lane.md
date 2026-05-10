# 062 - Task Status Record And Active Run Model Strict Lane

Roadmap: [`g04.020`](../roadmaps/g04/020-task-status-record-and-active-run-model.md)

Status: Active
Owner: Platform
Created: 2026-05-10

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

## Current Ready Card

- [`618-promote-task-status-identity-persistence-and-state-model-boundary.md`](../roadmaps/g04/batch-cards/618-promote-task-status-identity-persistence-and-state-model-boundary.md)

## Execution Chain

- `617` complete: opened the lane, promoted the first task-status contract
  anchor, and selected the first contract-shaping card
- `618` complete: locked task-status identity, state/stage taxonomy, active/completed
  persistence layout, and stale-record reconciliation boundary before
  implementation
- `619` ready: add typed task-status key/record types and runtime/report path
  helpers before write-side execution hooks

## Exit Condition

This lane is complete when Effigy has one canonical task-status record model,
one shared write-side status owner across task execution surfaces, and enough
proof coverage that the later read/query lane can trust the persisted status
truth.

## Next Task

Execute ready card
[`619-add-task-status-record-types-and-path-helpers.md`](../roadmaps/g04/batch-cards/619-add-task-status-record-types-and-path-helpers.md).
