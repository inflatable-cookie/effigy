# 497 - Select Shared Service Or Generated Image Cleanup Migration

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next bounded manager-backed migration slice after gateway TCP alias
host updates.

## Scope

- inventory remaining direct compose/backend calls after card `496`
- choose one next slice:
  - shared-service bring-up helpers
  - generated image cleanup during reset
- update lane and roadmap front doors
- do not implement code in this decision card

## Non-Goals

- no public CLI behavior changes
- no code migration
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when one next implementation card is ready.

## Closeout

The next slice is shared-service bring-up.

Only two direct-call clusters remain outside adapter modules:

- shared-service compose `up -d`
- generated image cleanup during reset

Shared services come first because they are still compose-backed operation
work. Generated image cleanup is a runtime command adapter slice after that.

## Validation

- `git diff --check`

## Next Task

Wire manager compose plans into shared-service bring-up.
