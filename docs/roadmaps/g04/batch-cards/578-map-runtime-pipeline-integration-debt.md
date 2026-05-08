# 578 - Map Runtime Pipeline Integration Debt

Lane: [`054-runtime-pipeline-integration-audit-and-debt-map-strict-lane.md`](../054-runtime-pipeline-integration-audit-and-debt-map-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Create the concrete debt map for `g04.012`.

## Scope

- scan runtime/container drift guard allowances and classify each one
- scan activation-plan callers and duplicated builders
- scan data seed/dump plan usage versus low-level helper usage
- scan container operation plan usage and discarded plans
- scan volume inventory and orphan filtering integration points
- scan QA aggregators for architecture guard coverage
- scan large planning crates and select decomposition candidates
- choose the first implementation roadmap/card

## Non-Goals

- no behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the strict lane contains a debt table and selected
implementation order.

## Validation

- `bash scripts/check-runtime-container-drift.sh`
- docs path/link checks for changed planning docs
- `git diff --check`

## Next Task

Start
[`579-open-runtime-activation-route-authority-lane.md`](./579-open-runtime-activation-route-authority-lane.md).
