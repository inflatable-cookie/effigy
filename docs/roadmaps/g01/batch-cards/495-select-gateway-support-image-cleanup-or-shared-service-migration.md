# 495 - Select Gateway Support Image Cleanup Or Shared Service Migration

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next bounded manager-backed migration slice after `container up`.

## Scope

- inventory remaining direct compose/backend calls after card `494`
- choose one next slice:
  - gateway/support compose calls
  - generated image cleanup during reset
  - shared-service bring-up helpers
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

The next slice is gateway TCP alias host updates.

Remaining direct-call drift is now mostly:

- gateway TCP alias host update `compose exec`
- shared-service bring-up helpers
- generated image cleanup during reset

Gateway support comes first because it is a narrow runner-owned `compose exec`
path and can move behind manager-owned compose plans without changing gateway
route selection or rendering.

## Validation

- `git diff --check`

## Next Task

Wire manager compose plans into gateway TCP alias host updates.
