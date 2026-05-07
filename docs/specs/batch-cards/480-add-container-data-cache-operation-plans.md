# 480 - Add Container Data Cache Operation Plans

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Extend `effigy-container-ops` with operation plans for container data and cache
surfaces.

## Scope

- add data/cache operation variants to `ContainerOperationKind`
- support:
  - data list/export/import/pull-production/seed/dump
  - cache list/prune
- model destructive cache/data operations with explicit side-effect and
  confirmation policy
- add pure planning tests for transfer paths and destructive policy

## Non-Goals

- no runner migration yet
- no backend-manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `effigy-container-ops` exposes data/cache operation
plans and focused tests pass.

## Closeout

Added data/cache operation planning for:

- data list/export/import/pull-production/seed/dump
- cache list/prune

The shared model now captures runtime data mutation, host data writes, cache
removal, and confirmation policy for destructive data/cache operations.

## Validation

- `cargo test -p effigy-container-ops`
- `git diff --check`

## Next Task

Wire data/cache operation plans into runtime and runner glue.
