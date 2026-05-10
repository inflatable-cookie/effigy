# 623 - Promote Task Status Query Scope And Result Contract

Lane: [`063-task-status-query-surface-and-read-model-strict-lane.md`](../063-task-status-query-surface-and-read-model-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Goal

Lock the read-side product boundary for `effigy tasks status` before parser and
report implementation start.

## Scope

- lock selector-resolution rules for `tasks status <selector>`
- lock repo-plus-descendant inventory rules for `tasks status --all`
- lock stale/no-longer-declared row visibility rules
- lock minimum text output expectations
- lock minimum JSON schema fields and ids
- confirm read-side ownership split between task discovery, runtime
  reconciliation, and report shaping

## Non-Goals

- no parser or dispatch implementation yet
- no machine-wide inventory
- no control verbs
- no cleanup or retention policy
- no release work

## Exit Condition

This card is complete when the query surface semantics are explicit enough that
implementation can proceed without reopening inventory scope or stale-row
visibility decisions.

## Validation

- docs path checks for changed roadmap/spec/contract surfaces
- `git diff --check`

## Next Task

Add parser/dispatch support for `effigy tasks status` once the query-surface
contract is locked.
