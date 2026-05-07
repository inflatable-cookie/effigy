# 008 - Manager Backed Runtime Read Write Shell

Generation: `g04`

Status: Active
Owner: Platform
Created: 2026-05-07
Depends on: [`007-effective-container-policy-decomposition.md`](./007-effective-container-policy-decomposition.md)

## Goal

Remove remaining old compose/process command construction from `effigy-runtime`.

## Scope

- move read/write/shell direct calls behind `ContainerManager`
- split runtime data/read/write/shell modules
- keep public runtime helper APIs stable until migrated callers no longer need
  them
- add drift guards for old compose/runtime helper usage

## Migration Targets

- `crates/effigy-runtime/src/data.rs`
- `crates/effigy-runtime/src/read.rs`
- `crates/effigy-runtime/src/write.rs`
- `crates/effigy-runtime/src/shell.rs`
- `crates/effigy-runtime/src/signals.rs`

## Acceptance Criteria

- runtime crate no longer calls `compose_args` directly outside manager adapters
- runtime crate no longer exposes Docker-named helpers
- operation reports remain compatible

## Validation

- `cargo test -p effigy-runtime`
- focused status/logs/reset/data/cache tests

## Next Task

Start card
[`554-extract-runtime-data-transfer-validation.md`](../../specs/batch-cards/554-extract-runtime-data-transfer-validation.md).
