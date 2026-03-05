# Roadmap 012 Batch 12.3 Checkpoint (Managed Runtime Separation)

Date: 2026-03-01
Roadmap: [g01.012 - Codebase Consolidation and Health](../../roadmaps/g01/012-codebase-consolidation-and-health.md)

## Scope

Decompose `src/runner/managed.rs` into focused submodules for task-reference resolution, runtime execution, and DAG scheduling/policy rendering while preserving behavior.

## Changes

- Extracted task reference resolution and builtin invocation rendering:
  - `src/runner/managed/references.rs`
- Extracted runtime execution paths:
  - `src/runner/managed/runtime.rs`
- Extracted DAG scheduling and policy wrappers:
  - `src/runner/managed/scheduler.rs`
- Reduced `src/runner/managed.rs` to core plan resolution + run-spec rendering orchestration.

## Validation

Executed:
- `cargo check`
- `cargo test --lib run_manifest_task_managed -- --nocapture`
- `cargo test --lib managed_stream -- --nocapture`

Result:
- compile passed
- managed-focused tests passed (29 + 8 tests in targeted runs)

## Notes

- Command outputs and error surfaces were preserved.
- This batch intentionally avoids user-facing schema/CLI changes.
