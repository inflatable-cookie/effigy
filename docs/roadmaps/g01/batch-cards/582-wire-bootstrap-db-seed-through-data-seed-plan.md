# 582 - Wire Bootstrap DB Seed Through Data Seed Plan

Lane: [`056-data-seed-dump-plan-consumption-strict-lane.md`](../056-data-seed-dump-plan-consumption-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make bootstrap DB seed execution consume a `DataSeedPlan`.

## Scope

- inspect current `DataSeedPlan` fields against bootstrap DB seed execution
- add missing seed-plan fields or constructors in `effigy-data` if needed
- build one `DataSeedPlan` in `src/runner/db_seed.rs` before staging/execution
- preserve local SQL and `oci://` seed behavior
- keep prompts and CLI rendering in runner code
- add focused tests for seed source, target, artifact handoff, and command plan

## Non-Goals

- no container data seed migration yet
- no dump migration yet
- no artifact transport rewrite
- no public behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when bootstrap DB seed no longer reaches directly for
all low-level `effigy-data` helper decisions independently of a seed plan.

## Validation

- `cargo test -p effigy-data`
- `cargo test -p effigy --lib db_seed -- --test-threads=1`
- `git diff --check`

## Next Task

Start
[`583-confirm-container-data-seed-plan-convergence.md`](./583-confirm-container-data-seed-plan-convergence.md).
