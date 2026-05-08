# 419 - Seed Dump Apply Capture Integration

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-06
Completed: 2026-05-06

## Goal

Start routing seed/dump surfaces through the artifact model without changing
their public behavior for plain local SQL files.

## Scope

- add an artifact resolver boundary callable from bootstrap and container data
  paths
- preserve existing local `--db-seed target=path.sql[.gz]` behavior
- stage local artifact sources before handoff
- define where artifact metadata is passed into task execution
- draft apply/capture report integration for later UAT use
- keep OCI network transport out of this card unless a local fixture path needs
  it

## Non-Goals

- no public artifact command yet
- no live private registry proof
- no Acowtancy file edits
- no migration semantics in Effigy

## Exit Condition

This card is complete when at least one seed path resolves through
`effigy-artifacts` with behavior preserved for existing local SQL inputs, and
the remaining seed/dump surfaces have precise follow-up hooks.

## Closeout

Completed the first seed integration:

- root crate depends on `effigy-artifacts`
- `stage_db_seed_files(...)` now stages each local seed through
  `stage_local_artifact(...)`
- the legacy `.effigy/local/db-seeds` handoff is preserved by copying from the
  staged artifact payload into the old seed location
- a focused unit test asserts both the legacy staged seed path and the new
  `effigy-artifact.json` metadata

Validation passed:

- `cargo test -p effigy-artifacts` passes
- `cargo test -p effigy --lib db_seed` passes

## Next Task

Card [`420-acowtancy-proof-and-closeout.md`](./420-acowtancy-proof-and-closeout.md).
