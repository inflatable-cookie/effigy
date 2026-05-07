# 003 - Runtime Activation Pipeline

Generation: `g04`

Status: Active
Owner: Platform
Created: 2026-05-07
Depends on: [`002-execution-pipeline-ownership.md`](./002-execution-pipeline-ownership.md)

## Goal

Move runtime prep into a typed activation pipeline.

## Scope

- add `crates/effigy-runtime-plan`
- define activation request, plan, route, readiness, alias, lease, and report
  types
- move pure planning out of `container_runtime_prep`
- split side-effect steps into named stages
- make standard, managed, exec, workspace, bootstrap, and Rhai container paths
  consume the same activation pipeline

## Migration Targets

- `src/runner/container_runtime_prep/mod.rs`
- `src/runner/execute/pipeline/standard.rs`
- `src/runner/execute/pipeline/managed.rs`
- `src/runner/exec_command/*`
- `src/runner/system_command/workspace/*`
- `src/runner/db_seed.rs`

## Acceptance Criteria

- runtime prep is split into stage modules
- activation reports are testable without live containers
- no new caller-local activation booleans are introduced
- gateway, alias, readiness, and lease behavior remain equivalent

## Validation

- `cargo test -p effigy-runtime-plan`
- `cargo test -p effigy --lib container_runtime_prep`
- focused direct/container/bootstrap/Rhai activation tests

## Next Task

Start card
[`455-move-runtime-prep-activation-executor-behind-plan.md`](../../specs/batch-cards/455-move-runtime-prep-activation-executor-behind-plan.md).
