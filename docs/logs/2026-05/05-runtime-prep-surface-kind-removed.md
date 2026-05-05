# Runtime Prep Surface Kind Removed

Date: 2026-05-05

## Summary

Removed the runner-only `ExecutionSurfaceKind` bridge from runtime prep.

## Outcome

- `ActivationRequest` no longer carries a duplicate execution-surface label.
- Standard routed tasks, bootstrap, deferral, and explicit exec activation
  callers now pass only container name, repo override, and session context.
- Runtime-prep tests assert shared activation side effects through actual
  behavior: prepare, gateway readiness, and host-container lease refresh.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy container_runtime_prep -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy exec_command -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy execute::pipeline::standard -- --nocapture`
- `rg "ExecutionSurfaceKind" src/runner -n`
- `rg "surface: ExecutionSurfaceKind|ActivationRequest \{[^}]*surface" src/runner -n`

## Next

Card `393` decides the next `g03.033` cleanup target.
