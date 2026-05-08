# 506 - Move Logical Data Target Model into Effigy Data

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move the shared logical data target shape out of runner DB seed code.

## Scope

- replace runner-local `LogicalDatabaseTarget` with `effigy-data`
  `ResolvedDataTarget` where practical
- keep target collection in runner for now if manifest dependency would make
  `effigy-data` too heavy
- migrate DB seed and DB dump target resolution to use the shared target model
- preserve current target names, database names, sidecar service mapping, and
  error text unless a test exposes a real inconsistency

## Non-Goals

- no full manifest resolver inside `effigy-data` yet
- no artifact staging migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when DB seed and dump no longer share target identity via
a runner-private struct.

## Closeout

Replaced the runner-private `LogicalDatabaseTarget` with
`effigy_data::ResolvedDataTarget`. Manifest target collection remains in the
runner to keep `effigy-data` dependency-light, but DB seed and dump now share
the data crate's target identity model.

## Validation

- `cargo test -p effigy-data` passed
- `cargo test -p effigy --lib container_command::data` passed
- `cargo test -p effigy --lib db_seed` passed
- `git diff --check` passed

## Next Task

Start card
[`507-move-seed-source-normalization-into-effigy-data.md`](./507-move-seed-source-normalization-into-effigy-data.md).
