# 571 - Add Exec Workspace Managed Proof Coverage

Lane: [`052-drift-guards-and-architecture-proof-matrix-strict-lane.md`](../052-drift-guards-and-architecture-proof-matrix-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Add focused proof coverage for the runtime/container surfaces whose existing
tests are useful but scattered.

## Scope

- add or tighten proof tests for `effigy exec` container activation and handoff
  transport boundaries
- add or tighten proof tests for workspace seeded/bootstrap handoff cleanup and
  activation-plan identity
- add or tighten proof tests for managed activation host/handoff lease-policy
  identity
- prefer pure planner/unit seams over live container boot
- update the proof matrix if a different gap is found while implementing

## Non-Goals

- no live external project mutation
- no broad QA run
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the selected `exec`, workspace, and managed proof
rows have focused tests or documented existing tests strong enough to satisfy
the matrix.

## Validation

- targeted `cargo test` commands for touched modules
- `bash scripts/check-runtime-container-drift.sh`
- `git diff --check`

## Next Task

Start
[`572-close-drift-guards-and-handoff-contract-promotion.md`](./572-close-drift-guards-and-handoff-contract-promotion.md).
