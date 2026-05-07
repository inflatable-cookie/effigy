# 516 - Add Data Service Selection Plan Foundation

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move shared database service matching rules into `effigy-data`.

## Scope

- add a dependency-light `DatabaseService` model
- add a pure database service selection helper
- support requested service lookup
- support service selection by declared database list
- support fallback selection by primary database
- preserve ambiguous and missing-service diagnostics through runner formatting
- migrate DB seed builtin service selection to use the helper
- migrate container data dump service selection to use the helper

## Non-Goals

- no manifest dependency in `effigy-data`
- no container execution changes
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when seed and dump no longer duplicate database service
matching rules in runner modules.

## Closeout

Added `DatabaseService`, `select_database_service`, and
`DatabaseServiceSelectionError` to `effigy-data`. DB seed and data dump still
adapt manifest service config locally, but the matching rules now live behind
the shared data pipeline boundary.

## Validation

- `cargo test -p effigy-data -- --test-threads=1` passed
- `cargo test -p effigy --lib container_command::data -- --test-threads=1`
  passed
- `cargo test -p effigy --lib runner::db_seed::tests -- --test-threads=1`
  passed
- `cargo check -p effigy --lib` passed with the existing
  `runtime_activation_report_for_result` dead-code warning

## Next Task

Start card
[`517-select-data-pipeline-closeout-or-runner-module-split.md`](./517-select-data-pipeline-closeout-or-runner-module-split.md).
