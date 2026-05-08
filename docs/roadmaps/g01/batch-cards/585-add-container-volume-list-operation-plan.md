# 585 - Add Container Volume List Operation Plan

Lane: [`057-container-volume-operation-pipeline-strict-lane.md`](../057-container-volume-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make `container volume list` a first-class container operation.

## Scope

- add `ContainerVolumeOperation`
- add `ContainerOperationKind::Volume`
- model `volume list` as read-only with orphan/profile filters
- create a runtime global volume operation plan before inventory collection
- add focused operation-plan tests

## Non-Goals

- no volume export/import reshaping
- no CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `container volume list` is represented by a typed
operation plan.

## Validation

- `cargo test -p effigy-container-ops`
- `cargo test -p effigy-runtime volume -- --test-threads=1`
- `git diff --check`

## Next Task

Start
[`586-wire-architecture-guard-into-validation-aggregators.md`](./586-wire-architecture-guard-into-validation-aggregators.md).
