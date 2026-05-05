# 384 - Migrate Container Lifecycle Through Manager

Lane: [`038-plugin-ready-container-manager-facade-strict-lane.md`](../038-plugin-ready-container-manager-facade-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

## Goal

Route container lifecycle commands through `ContainerManager` while preserving
current CLI behavior.

## Scope

- migrate `container up`, `down`, `status`, `stats`, and `logs` command
  construction through manager operations
- keep existing process execution helpers where they still own IO details
- preserve current attached `container up` behavior
- start moving interrupt closeout reporting into manager-owned reports
- add focused tests for lifecycle operation reports and invocation parity
- do not migrate exec/copy/data operations in this card

## Exit Condition

This card is complete when lifecycle command paths consume manager operations,
operation reports include the required identity fields, and remaining direct
backend branching is isolated to exec/copy/data follow-up cards.

## Next Task

Decide whether to migrate attached interrupt closeout deeper or move next to
exec/copy/data operations.
