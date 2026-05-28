# 034 - Shared Database Target Resolution

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-12
Depends on:
- [`033-post-release-reference-grade-cleanup-suite.md`](./033-post-release-reference-grade-cleanup-suite.md)

## Goal

Converge database service discovery and target selection behind one shared
domain surface used by dump, seed, state, and future migration/media flows.

## Evidence

- duplicate scan reported a 42-line duplicate block between
  `src/runner/container_command/data.rs` and `src/runner/db_seed.rs`
- both paths carry local helpers for database service kind, password lookup,
  declared databases, and primary database selection
- Example App migration work needs predictable DB target resolution before legacy
  import, state apply, media attachment, and deployment flows can be made boring

## Scope

- identify all current database target selection call sites
- promote a shared database service inventory and target-selection model
- choose the owning crate, expected to be `effigy-data` unless inspection proves
  a better home
- update seed and dump paths to consume the shared model
- make the shared model suitable for later state apply and migration bundle work
- preserve existing CLI, text, JSON, and error behavior unless explicitly
  documented

## Non-Goals

- no new database commands
- no provider database provisioning
- no schema migration framework changes
- no Example App-specific logic
- no media/object-store behavior in this lane

## Core Decisions

### Ownership

Database target resolution is domain logic, not container command glue. The
runner should ask for a resolved target, then orchestrate command execution.

### Public Shape

The shared model should describe:

- source container service
- database engine kind
- declared databases
- selected database
- credential reference source
- blockers and warnings when selection is ambiguous

### Compatibility

Current command behavior is the contract. The first implementation should add
tests that prove old seed/dump examples still resolve the same target.

## Acceptance Criteria

- `container data dump` and built-in DB seed no longer carry duplicate database
  service helper logic
- one shared target-resolution API is covered by focused unit tests
- ambiguous or missing DB service errors remain clear
- the shared API is documented enough for state/apply callers to adopt later
- duplicate-block scan no longer reports the seed/dump service helper block

## Outcome

- `effigy-data` now owns the manifest-neutral database service normalization
  helper
- runner code owns only the thin manifest TOML adapter
- `container data dump` and built-in DB seed now use the same shared service
  collection path
- the old seed/dump duplicate helper block is gone
- `catalog = "mysql"` is accepted consistently with `mariadb` as a
  MariaDB/MySQL service

## Suggested Batch Cards

- `657-open-shared-database-target-resolution-lane.md`
- `658-promote-database-target-resolution-boundary.md`
- `659-add-shared-database-target-model-and-tests.md`
- `660-migrate-seed-and-dump-to-shared-target-resolution.md`
- `661-close-database-target-resolution-docs-and-drift-check.md`

## Validation

- targeted `effigy-data` tests
- runner seed/dump tests
- `effigy scan duplicate-blocks --json`
- `effigy test --plan`
- `git diff --check`

## Next Task

Open `g04.035` state-domain extraction.
