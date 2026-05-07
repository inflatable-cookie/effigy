# 002 - Execution Pipeline Ownership

Generation: `g04`

Status: Complete
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
- standard and managed pipeline file-size work is handed to `g04.003` because
  the remaining bulk is runtime/container activation ownership
- equivalent direct/bootstrap/Rhai/run-array/demo task inputs produce equivalent
  route plans

## Closeout

Completed shared planning surfaces in `effigy-execution`:

- `ExecutionDispatchPlan`
- `ExecutionPreflightInput`
- `ExecutionRuntimeArgsPlan`
- `ExecutionDiscoveryPlan`
- `ExecutionSelectionPlan`
- `ExecutionBindingPlan`

Remaining standard/managed pipeline size is now blocked by runtime activation,
policy loading, workspace-seeded sessions, inline workspace cleanup, direct
compose calls, and managed session handling. That work moves to `g04.003`.

## Validation

- `cargo test -p effigy-execution`
- `cargo test -p effigy --lib execute`
- focused direct/bootstrap/Rhai/run-array/demo parity tests

## Next Task

Start card
[`444-scaffold-runtime-activation-pipeline-lane.md`](../../specs/batch-cards/444-scaffold-runtime-activation-pipeline-lane.md).
