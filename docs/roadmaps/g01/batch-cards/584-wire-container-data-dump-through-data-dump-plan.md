# 584 - Wire Container Data Dump Through Data Dump Plan

Lane: [`056-data-seed-dump-plan-consumption-strict-lane.md`](../056-data-seed-dump-plan-consumption-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make `container data dump` consume `DataDumpPlan`.

## Scope

- replace runner-local `DbDumpPlan` with `effigy-data::DataDumpPlan`
- move dump destination classification and artifact handoff into plan
  construction
- keep prompt/output rendering in `container_command/data.rs`
- preserve local SQL, `oci://`, and `--push` behavior
- update focused dump tests to assert the plan shape

## Non-Goals

- no artifact transport rewrite
- no public behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when dump execution reads target, destination, command,
and artifact capture intent from `DataDumpPlan`.

## Validation

- `cargo test -p effigy-data`
- `cargo test -p effigy --lib data_dump -- --test-threads=1`
- `git diff --check`

## Next Task

Start
[`585-add-container-volume-list-operation-plan.md`](./585-add-container-volume-list-operation-plan.md).
