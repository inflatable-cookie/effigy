# 493 - Select Gateway Support Image Cleanup Or Up Migration

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next bounded manager-backed migration slice after data
pull-production.

## Scope

- inventory remaining direct compose/backend calls after card `492`
- choose one next slice:
  - gateway/support compose calls
  - generated image cleanup during reset
  - `container up` bring-up
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

The next slice is `container up` bring-up.

Remaining direct-call drift is now mostly:

- `container up` attached and detached compose bring-up
- gateway TCP alias host updates
- generated image cleanup during reset
- shared-service bring-up helpers

`container up` comes first because it is the main lifecycle activation command
and still owns both attached and detached compose execution locally.

## Validation

- `git diff --check`

## Next Task

Wire manager compose plans into container up.
