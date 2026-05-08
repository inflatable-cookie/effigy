# 489 - Select Attached Session Or Data Cache Manager Migration

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next bounded manager-backed migration slice after exec/shell.

## Scope

- inventory remaining direct compose/backend calls after cards `487` and `488`
- choose one next slice:
  - attached `container up` and managed session handling
  - container data/cache transfer paths
  - gateway/support compose calls
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

The next slice is attached session migration.

Remaining direct-call drift after exec/shell is concentrated in:

- attached stream logs and closeout shutdown
- container data pull-production runtime bring-up
- gateway TCP alias host updates
- generated image cleanup during reset
- shared-service bring-up helpers

Attached session comes first because it owns Ctrl+C behavior and visible
container closeout, which is central to the manager abstraction goal.

## Validation

- `git diff --check`

## Next Task

Wire manager compose plans into attached session stream and closeout.
