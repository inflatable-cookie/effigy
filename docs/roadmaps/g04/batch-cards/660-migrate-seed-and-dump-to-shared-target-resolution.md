# 660 - Migrate Seed And Dump To Shared Target Resolution

Roadmap: [`../034-shared-database-target-resolution.md`](../034-shared-database-target-resolution.md)
Strict lane: [`../../../specs/070-shared-database-target-resolution-strict-lane.md`](../../../specs/070-shared-database-target-resolution-strict-lane.md)
Contract: [`../../../contracts/026-shared-database-target-resolution-contract.md`](../../../contracts/026-shared-database-target-resolution-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Remove the duplicated runner-local database service collection helpers by
feeding manifest service params into the shared `effigy-data` resolver.

## Scope

- add a small runner adapter from `ManifestContainerServiceConfig` to
  `DatabaseServiceManifestEntry`
- update `container data dump` service collection to use the shared helper
- update built-in DB seed service collection to use the shared helper
- remove duplicated runner-local password, declared database, primary database,
  and service-kind helpers
- preserve command-specific error wording where it is part of the current user
  experience

## Non-Goals

- no CLI behavior changes
- no JSON schema changes
- no state/media caller migration
- no provider database provisioning
- no Acowtancy-specific behavior

## Compatibility Decision

`effigy-data` treats `mysql` as `MariaDb`. The duplicate runner helpers did not
accept `mysql`. This card should either:

- accept `mysql` as a deliberate consistency fix and document it in
  `CHANGELOG.md` under `Fixed`, or
- preserve the old rejection by filtering the runner adapter and document why
  `effigy-data` remains wider than the current command callers.

Prefer the consistency fix unless tests reveal a real compatibility hazard.

## Acceptance

- seed and dump use the same shared service collection path
- duplicate helper functions are removed from both runner files
- focused seed/dump tests cover service selection through the new adapter
- `effigy scan duplicate-blocks --json` no longer reports the seed/dump helper
  block
- any accepted `mysql` behavior change has a changelog entry

## Outcome

- added a runner-local manifest adapter in `src/runner/db_services.rs`
- moved seed and dump service collection onto the shared `effigy-data`
  normalization path
- removed duplicated runner-local password, declared database, primary database,
  and catalog classification helpers
- accepted `catalog = "mysql"` as a consistency fix and recorded it in the
  changelog

## Validation

- `cargo test -p effigy-data` passed
- `cargo test db_services` passed
- `cargo check --bin effigy` passed
- `effigy scan duplicate-blocks --json` passed and no longer reports the
  seed/dump helper block
- `git diff --check`

## Next Task

Execute `661` to close docs, duplicate-scan, and drift proof for the lane.
