# 070 - Shared Database Target Resolution Strict Lane

Roadmap: [`g04.034`](../roadmaps/g04/034-shared-database-target-resolution.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Remove the current seed/dump database-service split path and promote database
target resolution into one shared domain seam before state, media, and
Acowtancy migration work build on the duplicated behavior.

## Hard Boundaries

- no CLI grammar changes
- no JSON schema changes unless a later card explicitly scopes them
- no provider provisioning
- no schema migration framework changes
- no media/object-store implementation
- no Acowtancy-specific logic
- no `.github/workflows/` edits
- no release execution

## Ownership Boundary

Database target resolution is domain behavior. Command modules may orchestrate
dump, seed, state, or future migration execution, but they should not each own
their own interpretation of:

- service kind
- declared databases
- primary database fallback
- credential source
- missing or ambiguous target errors

The expected home is `effigy-data` unless implementation evidence proves a
different existing domain crate is a better fit.

## Required Model

The shared target model should be able to represent:

- resolved service name
- database engine kind
- selected database name
- declared database inventory
- credential reference source
- blockers
- warnings

It should not expose secret values in reports, debug strings, or JSON payloads.

## Execution Chain

- `657` complete: opened the lane, added the strict-lane and contract anchors,
  and selected the first implementation slice
- `658` complete: promoted the database target resolution boundary after call-site
  mapping
- `659` complete: added the shared database target model and focused tests
- `660` complete: migrated seed and dump callers onto the shared resolver
- `661` complete: closed docs, duplicate-scan, and drift proof for the lane

## Exit Condition

This lane is complete when seed and dump no longer duplicate database service
resolution helpers, the shared resolver is covered by domain tests, existing
command behavior remains stable, and the seam is ready for later state/media
callers.

## Next Task

No active next task in this lane. Open `g04.035` state-domain extraction.
