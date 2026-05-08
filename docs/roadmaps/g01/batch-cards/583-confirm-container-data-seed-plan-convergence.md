# 583 - Confirm Container Data Seed Plan Convergence

Lane: [`056-data-seed-dump-plan-consumption-strict-lane.md`](../056-data-seed-dump-plan-consumption-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Confirm `container data seed` uses the same `DataSeedPlan` path as bootstrap
DB seed.

## Scope

- verify `container data seed` reaches seed staging and execution through
  `stage_db_seed_files` / `run_db_seed_task`
- add or adjust focused coverage if the existing tests do not prove the
  convergence
- keep prompting and confirmation rendering in `container_command/data.rs`
- select the first dump-plan migration card

## Non-Goals

- no dump implementation in this card
- no prompt or confirmation behavior changes
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `container data seed` has no separate seed planning
path to migrate.

## Validation

- `cargo test -p effigy --lib container_data_seed -- --test-threads=1`
- `cargo test -p effigy --lib db_seed -- --test-threads=1`
- `git diff --check`

## Next Task

Start
[`584-wire-container-data-dump-through-data-dump-plan.md`](./584-wire-container-data-dump-through-data-dump-plan.md).
