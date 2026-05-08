# 504 - Move Database Command Rendering into Effigy Data

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move postgres/mariadb seed and dump command rendering into `effigy-data`.

## Scope

- add pure command renderer helpers to `effigy-data`
- render dump commands for postgres and mariadb
- render builtin seed reset/import commands for postgres and mariadb
- migrate `src/runner/container_command/data.rs` dump command rendering to the
  shared helper
- migrate `src/runner/db_seed.rs` builtin seed reset/import rendering to the
  shared helper
- keep command argv identical unless a test exposes an existing inconsistency

## Non-Goals

- no logical target resolution migration yet
- no artifact staging migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when DB command argv construction is no longer locally
owned by the runner data modules.

## Closeout

Added postgres/mariadb dump and builtin seed command renderers to `effigy-data`
and migrated the runner DB seed/dump call sites to use them. Existing argv
shapes are preserved by focused package and module tests.

## Validation

- `cargo test -p effigy-data` passed
- `cargo test -p effigy --lib container_command::data` passed
- `cargo test -p effigy --lib db_seed` passed
- `git diff --check` passed

## Next Task

Start card
[`505-centralize-data-artifact-reference-classification.md`](./505-centralize-data-artifact-reference-classification.md).
