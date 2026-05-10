# 623 - Promote Task Status Query Scope And Result Contract

Lane: [`063-task-status-query-surface-and-read-model-strict-lane.md`](../063-task-status-query-surface-and-read-model-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-10
Completed: 2026-05-10

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

## Closeout

The query-surface contract is now explicit about:

- one-selector routing semantics
- repo-plus-descendant `--all` inventory scope
- the distinction between `unknown` declared rows and unresolved stale rows
- stale active fallback behavior
- minimum single-selector and `--all` row fields

## Next Task

Execute
[`624-add-tasks-status-parser-and-single-selector-dispatch.md`](./624-add-tasks-status-parser-and-single-selector-dispatch.md).
