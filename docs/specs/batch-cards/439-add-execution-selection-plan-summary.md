# 439 - Add Execution Selection Plan Summary

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Add a lifetime-light selection input/result summary to `effigy-execution`
without moving borrowed manifest selection out of runner.

## Scope

- add `ExecutionSelectionInput`
- add `ExecutionSelectionPlan`
- add selected catalog/task summary fields:
  - selector
  - invocation cwd
  - resolved root
  - catalog alias/root/manifest path/depth
  - selection mode
  - evidence
  - task name
- make runner selection build the shared summary after
  `select_catalog_and_task`
- keep `TaskSelection<'a>` in runner for actual dispatch
- add focused tests for explicit prefix, cwd-nearest, root-shallowest, and
  evidence preservation where existing fixtures make that cheap

## Non-Goals

- no `LoadedCatalog` ownership migration
- no `TaskSelection<'a>` ownership migration
- no fallback execution migration
- no binding migration
- no runtime activation migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when successful runner task selection produces a shared
selection plan summary and existing dispatch still uses the borrowed runner
selection.

## Validation

- `cargo test -p effigy-execution`
- `cargo test -p effigy --lib execute`
- `git diff --check`

## Closeout

Added lifetime-light selection input and plan summary types to
`effigy-execution`. Runner selection now builds an `ExecutionSelectionPlan`
after successful routing while keeping the borrowed `TaskSelection<'a>` for
managed and standard dispatch.

No fallback, binding, runtime activation, catalog ownership, or public CLI
behavior changed.

## Next Task

Start card
[`440-select-binding-input-or-selected-task-adapter-slice.md`](./440-select-binding-input-or-selected-task-adapter-slice.md).
