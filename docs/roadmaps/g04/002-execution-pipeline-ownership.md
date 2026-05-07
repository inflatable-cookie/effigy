# 002 - Execution Pipeline Ownership

Generation: `g04`

Status: Active
Owner: Platform
Created: 2026-05-07
Depends on: [`001-runtime-architecture-sanity-audit-and-generation-rollover.md`](./001-runtime-architecture-sanity-audit-and-generation-rollover.md)

## Goal

Make `effigy-execution` the real execution planning authority, not only the
request-shape crate.

## Scope

- add full dispatch/preflight/binding plan types
- move pure preflight planning out of runner where dependencies allow
- keep side-effectful dispatch in runner until the planning surface is stable
- make direct CLI, bootstrap, Rhai, run-array, demo, and deferral consume
  equivalent plan objects
- reduce `src/runner/execute/pipeline/standard.rs`
- reduce `src/runner/execute/pipeline/managed.rs`

## Migration Targets

- `src/runner/execute/entry.rs`
- `src/runner/execute/api.rs`
- `src/runner/execute/planning.rs`
- `src/runner/execute/binding.rs`
- `src/runner/execute/routing.rs`
- `src/runner/execute/pipeline/standard.rs`
- `src/runner/execute/pipeline/managed.rs`
- `crates/effigy-execution/src/lib.rs`

## Acceptance Criteria

- `run_manifest_task_request` consumes a resolved execution plan
- embedded task dispatch cannot bypass request construction
- standard and managed pipeline files either shrink below 500 lines each or
  split into clear owner modules
- equivalent direct/bootstrap/Rhai/run-array/demo task inputs produce equivalent
  route plans

## Validation

- `cargo test -p effigy-execution`
- `cargo test -p effigy --lib execute`
- focused direct/bootstrap/Rhai/run-array/demo parity tests

## Next Task

Start card
[`434-select-next-execution-planning-slice.md`](../../specs/batch-cards/434-select-next-execution-planning-slice.md).
