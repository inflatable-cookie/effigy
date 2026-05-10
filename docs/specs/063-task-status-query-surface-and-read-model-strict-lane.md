# 063 - Task Status Query Surface And Read Model Strict Lane

Roadmap: [`g04.021`](../roadmaps/g04/021-task-status-query-surface-and-read-model.md)

Status: Active
Owner: Platform
Created: 2026-05-10

## Purpose

Expose one read-only task-status query surface on top of the completed
task-status record model from `g04.020`.

This lane owns:

- `effigy tasks status <selector>`
- `effigy tasks status --all`
- text/JSON query contracts
- repo-plus-descendant inventory behavior

## Hard Boundaries

- keep status query repo-scoped, not machine-global
- do not add stop, restart, unlock, or tail verbs
- do not widen into non-task built-in status
- keep read ownership below CLI glue
- no `.github/workflows/` edits
- no release execution

## Current Ready Card

- [`625-add-tasks-status-all-repo-and-descendant-inventory.md`](../roadmaps/g04/batch-cards/625-add-tasks-status-all-repo-and-descendant-inventory.md)

## Execution Chain

- `622` complete: opened the lane, promoted the query-surface contract anchor,
  and selected the first contract-shaping card
- `623` complete: locked selector resolution, `--all` repo inventory scope,
  stale/no-longer-declared row visibility, and minimum text/JSON result fields
- `624` complete: `effigy tasks status <selector>` now resolves one selector
  through normal routing and reports live-or-last-known status in text or JSON
- `625` ready: widen the read surface to `tasks status --all`
  repo-plus-descendant inventory

## Exit Condition

This lane is complete when Effigy can answer task status for one selector or
the current repo/descendant inventory through one stable read model and one
stable text/JSON contract set.

## Next Task

Execute ready card
[`625-add-tasks-status-all-repo-and-descendant-inventory.md`](../roadmaps/g04/batch-cards/625-add-tasks-status-all-repo-and-descendant-inventory.md).
