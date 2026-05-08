# 491 - Select Data Cache Or Gateway Support Manager Migration

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next bounded manager-backed migration slice after attached session
handling.

## Scope

- inventory remaining direct compose/backend calls after card `490`
- choose one next slice:
  - container data/cache runtime calls
  - gateway/support compose calls
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

The next slice is container data pull-production runtime bring-up.

Remaining direct-call drift is now mostly:

- data pull-production `compose up`
- gateway TCP alias host updates
- generated image cleanup during reset
- `container up` attached/detached bring-up
- shared-service bring-up helpers

Data pull-production comes next because it is a focused data/cache runtime path
and can move to manager-owned compose plans without changing prompts, hooks,
gateway registration, or reports.

## Validation

- `git diff --check`

## Next Task

Wire manager compose plans into data pull-production bring-up.
