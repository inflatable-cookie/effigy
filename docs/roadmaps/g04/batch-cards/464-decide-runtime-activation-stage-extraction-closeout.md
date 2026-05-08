# 464 - Decide Runtime Activation Stage Extraction Closeout

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Decide whether the first runtime activation stage extraction slice can close or
needs one more implementation card.

## Scope

- review `src/runner/container_runtime_prep/mod.rs`
- compare extracted stage functions against `RuntimeActivationPlan::stages`
- decide one next move:
  - close `g04.003` stage-extraction slice and move to another activation
    caller
  - split `container_runtime_prep` into stage modules
  - add activation report plumbing to command surfaces
  - widen runtime activation planning to workspace/bootstrap/Rhai paths
- create the next ready card
- do not implement code in this decision card

## Non-Goals

- no code migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the next implementation card is ready and scoped.

## Closeout

Decision:

- close the first side-effect stage extraction slice
- next split `container_runtime_prep` into stage-owned modules

Rationale:

- current runtime prep now has named functions for each stage in
  `RuntimeActivationPlan::stages`
- keeping those stage functions in one file recreates the ownership problem the
  lane is meant to remove
- splitting modules before widening to workspace/bootstrap/Rhai keeps the next
  caller migrations smaller and easier to inspect

## Validation

- `git diff --check`

## Next Task

Split runtime prep into stage-owned modules.
