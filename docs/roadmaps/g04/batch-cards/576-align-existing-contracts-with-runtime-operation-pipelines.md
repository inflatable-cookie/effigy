# 576 - Align Existing Contracts With Runtime Operation Pipelines

Lane: [`053-contract-promotion-and-g04-closeout-strict-lane.md`](../053-contract-promotion-and-g04-closeout-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Update the existing runtime/container contracts so they name the current `g04`
pipeline owners and reference contract `015`.

## Scope

- update `005-container-runtime-contract.md`
- update `009-execution-surface-convergence.md`
- update `012-container-manager-contract.md`
- update `013-task-execution-request-contract.md`
- update `014-artifact-substrate-contract.md`
- keep edits focused on current owners, drift guards, and proof references

## Non-Goals

- no public behavior changes
- no changelog entry unless a real public behavior change is discovered
- no broad docs rewrites
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the existing contracts point at the current package
map and runtime operation pipeline contract without stale owner claims.

## Validation

- docs path/link checks for changed contracts
- `bash scripts/check-runtime-container-drift.sh`
- `git diff --check`

## Next Task

Start
[`577-close-g04-contract-promotion.md`](./577-close-g04-contract-promotion.md).
