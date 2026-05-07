# 017 - Planning Crate Decomposition

Generation: `g04`

Status: Queued
Owner: Platform
Created: 2026-05-07
Depends on:
- [`013-runtime-activation-route-and-plan-authority.md`](./013-runtime-activation-route-and-plan-authority.md)
- [`014-data-seed-dump-plan-consumption.md`](./014-data-seed-dump-plan-consumption.md)
- [`015-container-volume-operation-pipeline.md`](./015-container-volume-operation-pipeline.md)

## Goal

Split the new planning crates once their integration boundaries are proven, so
the modularisation does not recreate single-file god crates.

## Scope

- split `effigy-data` by target resolution, seed planning, dump planning,
  database command rendering, and artifact handoff
- split `effigy-container-ops` by lifecycle, read, exec/shell, data/cache,
  volume, side-effect classification, and reports
- split `effigy-execution` by request, environment, route, preflight,
  selection, binding, dispatch, and diagnostics
- split `effigy-artifacts` by refs, metadata, staging, OCI adapter, apply,
  capture, and reports
- split `effigy-container-manager` by backend ids, registry, request/report,
  compose invocation plans, and backend implementations
- keep public exports stable

## Non-Goals

- no behavior rewrites during mechanical decomposition
- no public CLI changes
- no release work
- no `.github/workflows/` edits

## Acceptance Criteria

- no replacement module becomes a new mixed-ownership hotspot
- public crate exports remain stable
- file-size improvements follow ownership boundaries, not arbitrary cuts
- package map reflects the new module owners

## Validation

- `cargo test -p effigy-data`
- `cargo test -p effigy-container-ops`
- `cargo test -p effigy-execution`
- `cargo test -p effigy-artifacts`
- `cargo test -p effigy-container-manager`
- `git diff --check`

## Next Task

Start after the integration roadmaps prove the crate seams.
