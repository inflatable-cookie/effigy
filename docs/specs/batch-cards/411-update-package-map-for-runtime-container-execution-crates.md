# 411 - Update Package Map For Runtime Container Execution Crates

Lane: [`041-contract-promotion-public-cleanup-breaks-and-closeout-strict-lane.md`](../041-contract-promotion-public-cleanup-breaks-and-closeout-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Update the live package map for `effigy-context`, `effigy-container-manager`,
and `effigy-execution`.

## Scope

- update `docs/architecture/010-package-map.md`
- name the new crates in the workspace crate map
- update runner ownership notes where the old owner descriptions are stale
- link the new contracts from the authority boundary
- no implementation changes

## Exit Condition

This card is complete when the package map reflects the shipped crate ownership
for runtime context, container manager facade, and task execution request
planning.

## Closeout

Updated `docs/architecture/010-package-map.md` with the new contract anchors and
crate owners for `effigy-context`, `effigy-container-manager`, and
`effigy-execution`.

Also tightened runner and lower-level container ownership descriptions so the
map no longer presents backend selection or task request construction as
caller-local ownership.

## Next Task

Widen the container runtime contract for manager and context ownership.
