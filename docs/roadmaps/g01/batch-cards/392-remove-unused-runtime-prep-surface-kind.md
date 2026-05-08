# 392 - Remove Unused Runtime Prep Surface Kind

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Remove the unused runner-only `ExecutionSurfaceKind` bridge from runtime prep.

## Scope

- delete `ExecutionSurfaceKind`
- remove `surface` from `ActivationRequest`
- update standard, deferral, bootstrap, and explicit exec activation callers
- update runtime-prep tests so they prove behavior through lease policy, not
  surface labels
- keep runtime behavior unchanged

## Exit Condition

This card is complete when runtime prep no longer carries a duplicate
execution-surface enum and focused runtime-prep, standard, deferral, and exec
tests pass.

## Closeout

`ExecutionSurfaceKind` was removed from runtime prep. `ActivationRequest` now
only carries the runtime data that activation actually consumes: container name,
repo override, and session context.

Tests now prove shared activation order and lease-refresh behavior directly
instead of repeating the same assertions through runner-local surface labels.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy container_runtime_prep -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy exec_command -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy execute::pipeline::standard -- --nocapture`
- `rg "ExecutionSurfaceKind" src/runner -n`
- `rg "surface: ExecutionSurfaceKind|ActivationRequest \{[^}]*surface" src/runner -n`

## Next Task

Decide the next post-surface cleanup boundary.
