# 572 - Close Drift Guards And Handoff Contract Promotion

Lane: [`052-drift-guards-and-architecture-proof-matrix-strict-lane.md`](../052-drift-guards-and-architecture-proof-matrix-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Close `g04.010` and hand off cleanly to `g04.011` contract promotion.

## Scope

- mark `g04.010` complete if its guard and proof-matrix acceptance criteria are
  satisfied
- move the `052` strict lane to recently completed status in the specs front
  door
- update roadmap front doors to point at `g04.011`
- scaffold the `g04.011` strict lane and first ready card if needed
- keep all existing drift allowances documented as migration debt

## Non-Goals

- no contract rewrites in this card
- no broad QA run
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `g04.010` is closed, no stale ready card points at
the drift-guard lane, and the next ready card belongs to `g04.011`.

## Validation

- `bash scripts/check-runtime-container-drift.sh`
- targeted tests from card `571` remain passing or are referenced as already
  run in this batch
- `git diff --check`

## Next Task

Start
[`573-scaffold-contract-promotion-closeout-lane.md`](./573-scaffold-contract-promotion-closeout-lane.md).
