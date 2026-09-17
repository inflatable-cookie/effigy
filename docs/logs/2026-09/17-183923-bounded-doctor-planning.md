# Bounded Doctor Planning

Status: complete
Created: 2026-09-17
Roadmap: g10.010
Batch: bounded-doctor-planning

## Summary

- Reconciled current doctor execution against a large catalogued monorepo.
- Promoted one bounded implementation task for a fast structural default,
  explicit deep diagnosis, shared catalog-scoped inventory, exact incremental
  cache, and visible deadline failure.
- Repaired stale g10 front doors after g10.008 and g10.009 completion.
- Prepared one ready-to-launch Queue handoff; created no worker or Queue task.

## Changes

- Added architecture `029`, contract `047`, and ready task `g10.010`.
- Set g10.010 as the sole approved frontier with no serial sibling.
- Preserved release, workflow, scan-policy, cache-pruning, and health-definition
  changes as explicit exclusions.

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`, `ROUTE`, `CONTRACT`.
- Movement: baseline unbounded composite doctor -> bounded structural default
  plus explicit, scoped, incremental deep diagnosis.
- Remaining gap: implementation, independent review, merge, and hook-owned
  closeout of g10.010.

## Validation Performed

- `effigy qa:docs`
- `git diff --check`
- semantic front-door and handoff checks

## Risks

- Deep health cancellation must own and reap the complete process tree.
- Exact cache invalidation cannot fall back to size and mtime.
- Shared inventory must preserve standalone scan findings and ordering.

## Next Task

- Dispatch ready task g10.010 through Northstar Queue.
