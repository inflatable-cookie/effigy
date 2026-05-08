# 014 - Data Seed Dump Plan Consumption

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-07
Depends on: [`012-runtime-pipeline-integration-audit-and-debt-map.md`](./012-runtime-pipeline-integration-audit-and-debt-map.md)

## Goal

Make `effigy-data` own seed/dump plans end to end, not just helper functions
used by runner glue.

## Scope

- promote `DataSeedPlan` and `DataDumpPlan` into the runner seed/dump flows
- move remaining target selection, source normalization, artifact handoff, and
  DB command rendering out of `src/runner/db_seed.rs` and
  `src/runner/container_command/data.rs`
- preserve existing bootstrap `--db-seed`, container data seed, local dump,
  and `oci://` dump behavior
- keep prompting and operator rendering in runner modules
- keep artifact transport/staging in `effigy-artifacts`
- reduce direct runner calls to low-level `database_*_command`,
  `seed_artifact_handoff`, and `dump_artifact_handoff` helpers

## Migration Targets

- `crates/effigy-data/src/lib.rs`
- `src/runner/db_seed.rs`
- `src/runner/container_command/data.rs`
- `src/runner/artifact_command.rs`
- `src/runner/artifact_transport.rs`

## Acceptance Criteria

- seed and dump execution consume full plan structs
- local SQL and `oci://` seed paths use equivalent plan shape
- local SQL and `oci://` dump paths use equivalent plan shape
- `container data dump --push` remains explicit and covered
- `db_seed.rs` and `container_command/data.rs` shrink materially or have
  clear remaining runner-only ownership

## Validation

- `cargo test -p effigy-data`
- targeted `db_seed` and `container_command::data` tests
- parser/help tests for seed/dump surfaces
- `git diff --check`

## Next Task

Continue with
[`g04.015`](./015-container-volume-operation-pipeline.md).
