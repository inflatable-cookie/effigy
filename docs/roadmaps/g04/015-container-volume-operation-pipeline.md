# 015 - Container Volume Operation Pipeline

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-07
Depends on: [`012-runtime-pipeline-integration-audit-and-debt-map.md`](./012-runtime-pipeline-integration-audit-and-debt-map.md)

## Goal

Fold the recent named-volume inventory and orphan filtering work into the
typed container operation pipeline.

## Scope

- add a container volume operation family to `effigy-container-ops`
- model `container volume list`, orphan filtering, and runtime profile
  inventory as typed operation plans
- decide whether volume export/import stay under data transfer or become
  volume operations with data aliases
- keep persistent-data reset safety explicit for `--keep-data` and
  `--wipe-data`
- route runtime volume capture through manager/runtime adapters where possible
- update help/parser tests only where terminology changes

## Migration Targets

- `crates/effigy-container-ops/src/lib.rs`
- `crates/effigy-runtime/src/data.rs`
- `crates/effigy-runtime/src/data/volumes.rs`
- `crates/effigy-catalog/src/volumes.rs`
- `src/runner/container_command/mod.rs`
- `src/runner/container_command/data.rs`
- `src/runner/container_command/support.rs`

## Acceptance Criteria

- `container volume list` has a first-class operation plan
- orphan filtering and all-profile inventory have focused tests
- reset keep-data/wipe-data behavior remains covered
- cache prune and volume inventory share classification helpers without
  conflating persistent data with purge-safe cache
- docs/contracts name the volume operation owner if the public surface changes

## Validation

- `cargo test -p effigy-container-ops`
- `cargo test -p effigy-runtime`
- targeted parser/help tests for container volume/data/cache surfaces
- `git diff --check`

## Next Task

Continue with
[`g04.016`](./016-architecture-guard-integration.md).
