# 581 - Close Runtime Activation Route Authority

Lane: [`055-runtime-activation-route-and-plan-authority-strict-lane.md`](../055-runtime-activation-route-and-plan-authority-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Close `g04.013` and hand off to data seed/dump plan consumption.

## Scope

- add a generic runtime-prep test proving non-task activation routes survive
  through `ActivationRequest`
- mark `g04.013` complete
- open the `g04.014` strict lane
- create the first data-plan consumption card
- update roadmap/spec front doors

## Non-Goals

- no data implementation in this card
- no manager/backend migration
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when route authority has proof coverage and the next
data-plan lane is ready.

## Validation

- `cargo test -p effigy --lib runtime_activation_plan -- --test-threads=1`
- docs path/link checks for changed planning docs
- `git diff --check`

## Next Task

Start
[`582-wire-bootstrap-db-seed-through-data-seed-plan.md`](./582-wire-bootstrap-db-seed-through-data-seed-plan.md).
