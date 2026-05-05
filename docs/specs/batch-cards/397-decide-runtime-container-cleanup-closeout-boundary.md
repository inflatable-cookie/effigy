# 397 - Decide Runtime Container Cleanup Closeout Boundary

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

## Goal

Decide whether `g03.033` is ready to close or needs one more bounded cleanup
slice.

## Scope

- review the `g03.033` exit condition
- inventory remaining direct cwd/backend/task-dispatch drift
- distinguish compatibility wrappers from brittle caller-local logic
- either create a final cleanup card or close the lane
- no implementation changes in this decision card

## Exit Condition

This card is complete when the lane either points at one final implementation
card or has an explicit closeout card.

## Next Task

Decide the `g03.033` closeout boundary.
