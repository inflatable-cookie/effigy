# 016 - Architecture Guard Integration

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-07
Depends on: [`012-runtime-pipeline-integration-audit-and-debt-map.md`](./012-runtime-pipeline-integration-audit-and-debt-map.md)

## Goal

Make the architecture drift guards part of normal validation instead of an
optional side task.

## Scope

- decide which aggregators should include `qa:architecture`
- likely wire `qa:architecture` into `qa:gates`, `qa:ci`, and `prepush:ci`
  without making day-to-day `qa` unexpectedly heavy
- add a guard for new large files over the agreed threshold with documented
  suppressions
- add a guard for new direct runner data/DB command rendering once
  `effigy-data` plan consumption lands
- add a guard for discarded operation plans if that pattern remains risky
- document suppression policy in active docs, not only old strict-lane text

## Migration Targets

- `tasks/effigy.tasks.toml`
- `scripts/check-runtime-container-drift.sh`
- `quality/effigy.scan.toml`
- docs under `docs/contracts/015-runtime-operation-pipeline-contract.md` or a
  small architecture guard guide

## Acceptance Criteria

- common validation catches runtime/container drift by default
- guard output remains fast and clear
- existing allowlisted debt is still explicit and path-scoped
- suppression process is documented

## Validation

- `effigy qa:architecture`
- selected aggregator dry-run or focused task execution
- `git diff --check`

## Next Task

Continue with
[`g04.017`](./017-planning-crate-decomposition.md).
