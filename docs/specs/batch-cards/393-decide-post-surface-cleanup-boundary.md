# 393 - Decide Post Surface Cleanup Boundary

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

## Goal

Choose the next concrete cleanup target in `g03.033` after removing the unused
runtime-prep surface bridge.

## Scope

- inspect the remaining `g03.033` cleanup targets
- compare remaining runner/container hotspots against already-migrated context,
  manager, and task-request surfaces
- choose one narrow implementation card
- avoid implementation changes in this decision card

## Exit Condition

This card is complete when the next cleanup target has a bounded write set and
the active lane points at its implementation card.

## Next Task

Decide whether the next cleanup should target runtime prep, standard/managed
pipeline glue, workspace provisioning, or container data operations.
