# 435 - Move Execution Preflight Input Behind Dispatch Plan

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make runner preflight consume the shared execution planning surface instead of
accepting a fresh `TaskInvocation` plus cwd pair.

## Scope

- add or refine `ExecutionPreflightInput` constructors in `effigy-execution`
- add a pure runtime-args planning helper around `ExecutionRuntimeArgsPlan`
- make `ExecutionDispatchPlan` expose the preflight input needed by runner
- update `src/runner/execute/entry.rs` so request-backed execution builds
  preflight from `ExecutionDispatchPlan`
- remove stale direct `run_manifest_task_with_cwd` entry wrappers once
  request-backed execution no longer needs them
- add focused tests for raw args, exec args, JSON mode, repo override, and cwd
  propagation

## Non-Goals

- no catalog discovery migration
- no task selection or binding migration
- no runtime activation migration
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when request-backed task execution reaches runner
preflight through `ExecutionDispatchPlan`/`ExecutionPreflightInput`, direct
wrapper behavior is unchanged, and focused tests cover runtime arg planning.

## Validation

- `cargo test -p effigy-execution`
- `cargo test -p effigy --lib execute`
- `git diff --check`

## Closeout

Moved runtime-arg planning into `effigy-execution` through
`ExecutionRuntimeArgsPlan`, added `ExecutionDispatchPlan::preflight_input()`,
and routed request-backed task execution into runner preflight through
`ExecutionPreflightInput`.

Runner discovery, catalog loading, selection, binding, managed dispatch, and
standard dispatch remain runner-owned. The old entry-local direct cwd wrappers
were removed because public callers already enter through request construction.

## Next Task

Start card
[`436-select-discovery-or-selection-planning-slice.md`](./436-select-discovery-or-selection-planning-slice.md).
