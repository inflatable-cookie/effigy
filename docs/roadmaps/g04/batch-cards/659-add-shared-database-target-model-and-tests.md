# 659 - Add Shared Database Target Model And Tests

Roadmap: [`../034-shared-database-target-resolution.md`](../034-shared-database-target-resolution.md)
Strict lane: [`../../../specs/070-shared-database-target-resolution-strict-lane.md`](../../../specs/070-shared-database-target-resolution-strict-lane.md)
Contract: [`../../../contracts/026-shared-database-target-resolution-contract.md`](../../../contracts/026-shared-database-target-resolution-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Add the shared database service inventory helper in `effigy-data` so seed and
dump can stop duplicating database-service interpretation.

## Scope

- add a manifest-neutral input shape for database service descriptors
- add a helper that converts those descriptors into `DatabaseService` values
- centralize catalog classification through `DatabaseServiceKind::from_catalog`
- cover password defaulting, declared database trimming, primary database
  trimming, unsupported catalog filtering, and `mysql` catalog behavior
- keep secret values out of debug/report-facing helper output where practical

## Non-Goals

- no runner caller migration in this card unless the change is trivial and
  isolated
- no CLI behavior changes
- no JSON schema changes
- no `effigy-manifest` dependency in `effigy-data` unless a blocker is found and
  documented
- no state/media caller adoption yet

## Implementation Notes

`effigy-data` currently has no dependencies. Preserve that if possible.

Preferred shape:

- runner extracts raw service name, catalog, password param, databases param,
  and database param from `ManifestContainerServiceConfig`
- `effigy-data` owns the typed normalization from those raw values into
  `DatabaseService`
- `mysql` should be treated consistently with
  `DatabaseServiceKind::from_catalog`, unless tests prove current runner
  rejection is intentional behavior

## Acceptance

- `effigy-data` has focused tests for the shared service inventory helper
- unsupported catalogs are ignored consistently
- password defaulting still matches current seed/dump behavior
- empty database entries are trimmed and ignored
- primary database extraction still matches current seed/dump behavior
- `660` can migrate seed and dump without changing selection semantics

## Outcome

- added `DatabaseServiceManifestEntry`
- added `collect_database_services_from_manifest_entries`
- centralized manifest-neutral service normalization in `effigy-data`
- covered trimming, default password behavior, unsupported catalogs, and `mysql`
  catalog handling with focused tests

## Validation

- `cargo test -p effigy-data` passed
- `git diff --check`

## Next Task

Execute `660` to migrate seed and dump callers onto the shared resolver.
