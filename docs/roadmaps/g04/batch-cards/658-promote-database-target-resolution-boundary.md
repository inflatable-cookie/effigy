# 658 - Promote Database Target Resolution Boundary

Roadmap: [`../034-shared-database-target-resolution.md`](../034-shared-database-target-resolution.md)
Strict lane: [`../../../specs/070-shared-database-target-resolution-strict-lane.md`](../../../specs/070-shared-database-target-resolution-strict-lane.md)
Contract: [`../../../contracts/026-shared-database-target-resolution-contract.md`](../../../contracts/026-shared-database-target-resolution-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Map the existing seed and dump database target resolution paths, then lock the
exact shared owner and API boundary before code extraction starts.

## Scope

- inspect current seed, dump, state, and migration-adjacent database target
  callers
- identify duplicated helper behavior and any real behavior differences
- confirm whether `effigy-data` is the correct owner
- update the contract if call-site evidence changes the required model
- select the first code batch for the shared resolver

## Non-Goals

- no code extraction in this card unless the evidence is trivial and explicitly
  folded into the next card
- no CLI behavior changes
- no JSON schema changes
- no media/object-store implementation
- no Acowtancy-specific logic

## Acceptance

- all current database target call sites are listed
- duplicated behavior is classified as identical, divergent, or dead
- the shared owner is selected with evidence
- any contract adjustment is made before implementation
- `659` can implement the shared model without another planning pass

## Outcome

- call sites are listed in contract `026`
- duplicated runner helper behavior is classified
- `effigy-data` is selected as the owner for pure service selection and typed
  target resolution
- the manifest-specific TOML extraction boundary remains adapter-owned unless
  implementation evidence justifies a dependency change
- the `mysql` catalog divergence is recorded as an explicit implementation
  decision for `659`

## Suggested Evidence Commands

```sh
rg "collect_.*seed|collect_.*dump|manifest_database_service_kind|service_primary_database|service_declared_databases|service_password" src crates tests
rg "database" src/runner crates/effigy-data crates/effigy-state crates/effigy-manifest
```

## Validation

- docs review
- `git diff --check`

## Next Task

Implement `659` with the shared database target model and focused tests.
