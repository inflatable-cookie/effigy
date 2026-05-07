# 503 - Scaffold Effigy Data Crate and Target Model

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Add the first dependency-light `effigy-data` crate with pure data seed/dump
planning types.

## Scope

- add `crates/effigy-data`
- add the crate to the workspace
- define initial public models:
  - `DataTargetRef`
  - `ResolvedDataTarget`
  - `DatabaseServiceKind`
  - `DataSeedInput`
  - `DataSeedPlan`
  - `DataDumpInput`
  - `DataDumpPlan`
  - `DataDumpDestination`
  - `DatabaseCommandPlan`
  - `ArtifactDataHandoff`
  - `DataOperationReport`
- add focused pure unit tests for target refs, database kind labels, and local
  versus OCI destination classification
- do not migrate runner callers yet

## Non-Goals

- no public CLI behavior changes
- no DB command rendering migration yet
- no artifact staging side effects
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `effigy-data` exists, compiles, and carries the
first pure seed/dump target model.

## Closeout

Added `crates/effigy-data` and registered it with the workspace. The crate now
owns the first pure seed/dump model layer: target refs, resolved targets,
database service kinds, seed sources, dump destinations, command plans, artifact
handoffs, and operation reports.

## Validation

- `cargo test -p effigy-data` passed
- `git diff --check` passed

## Next Task

Start card
[`504-move-database-command-rendering-into-effigy-data.md`](./504-move-database-command-rendering-into-effigy-data.md).
