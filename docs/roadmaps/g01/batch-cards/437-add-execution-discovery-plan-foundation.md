# 437 - Add Execution Discovery Plan Foundation

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Add the first shared discovery plan shape to `effigy-execution` while keeping
side-effectful context and catalog discovery in runner.

## Scope

- add `ExecutionDiscoveryInput`
- add `ExecutionDiscoveryPlan`
- add a selector planning helper around `effigy_tasks::parse_task_selector`
- make runner preflight convert discovery output into the shared plan shape
- keep `resolve_command_context_from_cwd` in runner
- keep `discover_catalogs_allow_missing` and loaded catalogs in runner
- keep `ExecutionPreflight` as the runner-local aggregate for this card
- add focused tests for selector parsing, repo override handoff, cwd, resolved
  root, and diagnostics/error text

## Non-Goals

- no task selection migration
- no catalog ownership migration
- no binding migration
- no runtime activation migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when runner discovery returns a shared
`ExecutionDiscoveryPlan`, preflight consumes that plan, and focused tests prove
selector/cwd/repo override behavior remains stable.

## Validation

- `cargo test -p effigy-execution`
- `cargo test -p effigy --lib execute`
- `git diff --check`

## Closeout

Added `ExecutionDiscoveryInput` and `ExecutionDiscoveryPlan` to
`effigy-execution`, moved selector parsing behind the shared execution planning
surface, and made runner preflight consume the shared discovery plan while
keeping context resolution and catalog loading in runner.

Catalog ownership, task selection, binding, managed dispatch, standard
dispatch, and runtime activation remain unchanged.

## Next Task

Start card
[`438-select-selection-input-or-catalog-handoff-slice.md`](./438-select-selection-input-or-catalog-handoff-slice.md).
