# 618 - Promote Task Status Identity Persistence And State Model Boundary

Lane: [`062-task-status-record-and-active-run-model-strict-lane.md`](../062-task-status-record-and-active-run-model-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Goal

Turn the first draft task-status contract into a settled implementation-ready
planning boundary.

## Scope

- lock the internal task-status identity key
- lock the normalized state and running-stage taxonomy
- define the minimum active and completed record fields
- define stale/live reconciliation rules for active records
- pin the first covered execution surfaces for the shared writer
- identify the smallest first implementation slice without opening final query
  UX

## Non-Goals

- no final `effigy tasks status` CLI output shape
- no write-side implementation yet
- no machine-wide inventory
- no restart, tail, or repair verbs
- no release work

## Exit Condition

This card is complete when the contract is specific enough that the first
implementation card can add typed task-status records and write-path hooks
without reopening identity, persistence, or stale-record semantics.

## Validation

- docs path checks for changed roadmap/spec/contract surfaces
- `git diff --check`

## Next Task

Choose the first implementation card for typed task-status records and shared
write-side hooks once this boundary is locked.
