# 547 - Scaffold Manager Backed Runtime Read Write Shell Lane

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Open `g04.008` with a concrete drift inventory and first implementation slice.

## Scope

- scan `effigy-runtime` for old compose/process construction
- record the first bounded migration target
- update strict-lane docs with the concrete inventory
- do not change runtime behavior yet

## Non-Goals

- no runtime helper migrations
- no container manager API expansion unless the inventory proves it is needed
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the lane records the current direct-call inventory
and names the next ready implementation card.

## Validation

- PASS: `git diff --check`

## Next Task

Rename runtime signal compose helpers away from Docker-specific names.
