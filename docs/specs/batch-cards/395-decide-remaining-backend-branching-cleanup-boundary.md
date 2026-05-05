# 395 - Decide Remaining Backend Branching Cleanup Boundary

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

## Goal

Choose the next bounded cleanup for backend-specific branching after container
inspection command shape moved behind `ContainerManager`.

## Scope

- inventory remaining direct backend checks in runner and container crates
- distinguish compatibility-layer branching from caller-local branching
- choose one narrow implementation card, or close this part of `g03.033` if
  only compatibility-layer branching remains
- no implementation changes in this decision card

## Exit Condition

This card is complete when the lane has a clear next card or an explicit
closeout decision for backend-branching cleanup.

## Next Task

Decide whether remaining backend branching should move, stay as compatibility
surface, or defer to `g03.035` contract cleanup.
