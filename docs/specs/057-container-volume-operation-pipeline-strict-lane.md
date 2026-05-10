# 057 - Container Volume Operation Pipeline Strict Lane

Roadmap: [`g04.015`](../roadmaps/g04/015-container-volume-operation-pipeline.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Purpose

Fold named-volume inventory into the typed container operation model.

## Hard Boundaries

- no release work
- no `.github/workflows/` edits
- no public CLI behavior changes
- keep volume inventory read-only
- keep cache pruning separate from persistent data classification

## Execution Chain

- `585` complete: add first-class `container volume list` operation planning

## Outcome

`ContainerVolumeOperation::list(...)` now exists in `effigy-containers`, and
runtime global volume listing creates a read-only volume operation plan before
collecting inventory.

## Exit Condition

This lane is complete because the recent `container volume list` surface has a
first-class operation plan and focused tests.

## Next Task

Card
[`586-wire-architecture-guard-into-validation-aggregators.md`](./batch-cards/586-wire-architecture-guard-into-validation-aggregators.md).
