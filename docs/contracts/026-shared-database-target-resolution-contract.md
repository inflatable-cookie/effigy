# Shared Database Target Resolution Contract

Generation: `g04`
Roadmap: [`../roadmaps/g04/034-shared-database-target-resolution.md`](../roadmaps/g04/034-shared-database-target-resolution.md)
Strict lane: [`../specs/070-shared-database-target-resolution-strict-lane.md`](../specs/070-shared-database-target-resolution-strict-lane.md)
Status: Draft
Owner: Platform
Updated: 2026-05-12

## Purpose

Define the structural boundary for resolving database services and database
targets across Effigy command surfaces.

This contract exists because seed and dump currently carry duplicated local
logic for discovering database services, selecting a target database, and
finding credential references. That behavior should be owned once before state,
media, and migration workflows rely on it.

## Hard Boundaries

- no command grammar changes
- no provider provisioning
- no secret creation or mutation
- no schema migration behavior changes
- no Example App-specific migration logic
- no object-store/media behavior
- no `.github/workflows/` edits
- no release execution

## Domain Boundary

The shared resolver owns database target selection. It may inspect manifest and
container policy data, but it must not perform command side effects.

The resolver owns:

- database service classification
- declared database inventory
- selected database calculation
- credential reference lookup
- missing service diagnostics
- ambiguous target diagnostics

The resolver does not own:

- Docker or container execution
- SQL import/export execution
- provider resource creation
- seed file discovery
- state-stack phase execution
- command text rendering

## Current Call-Site Evidence

The first audit found these concrete call sites:

- `src/runner/container_command/data.rs`: `resolve_db_dump_plans`,
  `collect_db_dump_services`, `resolve_db_dump_service_for_database`
- `src/runner/db_seed.rs`: `resolve_builtin_seed_service`,
  `collect_builtin_seed_services`
- `src/runner/bootstrap_command/mod.rs`: delegates bootstrap DB seed prompts to
  `db_seed`
- `effigy-data`: already owns `DatabaseService`, `DatabaseServiceKind`,
  `select_database_service`, data target selection, seed command plans, and dump
  command plans

The duplicate runner helpers are structurally identical for password extraction,
declared database extraction, primary database extraction, and service
collection.

One behavior difference must be resolved deliberately: `effigy-data`
`DatabaseServiceKind::from_catalog` accepts `postgres`, `mariadb`, and `mysql`,
while both runner-local duplicate helpers currently accept only `postgres` and
`mariadb`.

## Selected Owner

`effigy-data` remains the owner for pure database target and service selection.

Implementation should avoid adding an unnecessary `effigy-manifest` dependency
to `effigy-data` unless the dependency direction is explicitly accepted.
Preferred shape:

- `effigy-data` owns catalog classification, typed service inputs, service
  collection from manifest-neutral service descriptors, and selection errors
- runner or a thin adapter extracts TOML params from
  `ManifestContainerServiceConfig`
- seed and dump callers consume the same typed helper and keep only
  command-specific error wording where needed

## Required Output Shape

The first implementation should expose a typed result that can carry:

- selected service id
- database engine kind
- selected database id
- declared database ids
- credential reference source
- blockers
- warnings

Secret values must never be rendered, logged, or included in JSON output.

## Caller Rules

Seed, dump, and future state/media callers should consume the shared resolver
instead of duplicating service interpretation.

Callers may still own:

- command-specific flags
- command-specific fallback policy
- side-effect execution
- text rendering
- JSON envelope wrapping

## Compatibility Rules

The first migration must preserve existing seed and dump behavior unless a
roadmap card explicitly accepts a correction.

If existing duplicated paths disagree, implementation must stop and promote the
behavior decision before changing either caller.

## Acceptance Boundary

This contract is satisfied when:

- seed and dump use the same resolver
- resolver behavior is covered by focused domain tests
- missing and ambiguous target errors are clear
- no secret value can leak through reports or debug formatting
- later state-stack callers can adopt the resolver without depending on runner
  command modules
