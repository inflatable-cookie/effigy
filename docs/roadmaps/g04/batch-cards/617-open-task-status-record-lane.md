# 617 - Open Task Status Record Lane

Lane: [`062-task-status-record-and-active-run-model-strict-lane.md`](../062-task-status-record-and-active-run-model-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-10

## Goal

Open the next `g04` lane for task-status record truth after the state-stack
release slice closed.

## Scope

- activate `g04.020`
- create the `062` strict lane
- promote the first task-status contract anchor
- set the first ready card for contract shaping
- update roadmap/spec/contract front doors

## Non-Goals

- no task-status implementation yet
- no final `effigy tasks status` command UX yet
- no stop/restart/tail control surface
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the lane is open, the contract anchor exists, and
the first contract-shaping card is ready.

## Validation

- `cargo run --bin effigy -- docs check-paths docs/contracts/017-task-status-record-and-active-run-model-contract.md docs/roadmaps/g04/020-task-status-record-and-active-run-model.md docs/specs/062-task-status-record-and-active-run-model-strict-lane.md docs/roadmaps/g04/batch-cards/617-open-task-status-record-lane.md docs/roadmaps/g04/batch-cards/618-promote-task-status-identity-persistence-and-state-model-boundary.md docs/roadmaps/g04/README.md docs/specs/README.md docs/contracts/README.md`
- `git diff --check`

## Next Task

Card
[`618-promote-task-status-identity-persistence-and-state-model-boundary.md`](./618-promote-task-status-identity-persistence-and-state-model-boundary.md).
