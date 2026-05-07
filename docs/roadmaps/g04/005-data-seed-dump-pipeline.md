# 005 - Data Seed Dump Pipeline

Generation: `g04`

Status: Queued
Owner: Platform
Created: 2026-05-07
Depends on: [`004-container-operation-pipeline.md`](./004-container-operation-pipeline.md)

## Goal

Untangle database seed/dump/artifact capture planning from container command
glue.

## Scope

- add `crates/effigy-data`
- move logical DB target resolution out of `db_seed.rs` and
  `container_command/data.rs`
- move seed source normalization and artifact staging plan construction into
  `effigy-data`
- move dump target resolution and database command rendering into `effigy-data`
- keep task dispatch/container exec in runner/runtime pipeline
- preserve `--db-seed`, `--db-dump`, and `oci://` behavior

## Migration Targets

- `src/runner/db_seed.rs`
- `src/runner/container_command/data.rs`
- `src/runner/artifact_command.rs`
- `crates/effigy-artifacts/src/lib.rs`

## Acceptance Criteria

- `container_command/data.rs` no longer owns DB target resolution or dump
  command rendering
- `db_seed.rs` no longer owns artifact staging internals
- postgres/mariadb command rendering has focused unit tests
- `container data dump --push` remains explicit and covered

## Validation

- `cargo test -p effigy-data`
- `cargo test -p effigy --lib container_command::data`
- `cargo test -p effigy --lib db_seed`
- artifact seed/dump focused tests

## Next Task

Do not start until `g04.004` closes.
