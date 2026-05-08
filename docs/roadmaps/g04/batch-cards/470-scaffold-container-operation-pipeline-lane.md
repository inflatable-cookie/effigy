# 470 - Scaffold Container Operation Pipeline Lane

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Open `g04.004` with the first bounded implementation path for container
operation planning.

## Scope

- mark `g04.004` active
- define the first small `effigy-container-ops` crate boundary
- choose the first operation family to model
- create the next implementation card
- avoid code implementation in this scaffold card unless it is purely planning
  metadata

## Non-Goals

- no public CLI behavior changes
- no container command migration yet
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `g04.004` has one ready implementation card with a
small request/plan/report scope.

## Closeout

Selected the first implementation slice:

- add `crates/effigy-container-ops`
- start with lifecycle operation request/plan/report types
- model `up`, `down`, and `reset` before migrating runner code

Rationale:

- lifecycle operations carry the clearest safety policy and side-effect class
- data/exec/shell depend on the same operation identity shape, but are easier
  to migrate once lifecycle proves the substrate
- this keeps the first code card dependency-light and mostly pure planning

## Validation

- `git diff --check`

## Next Task

Add the `effigy-container-ops` lifecycle plan foundation.
