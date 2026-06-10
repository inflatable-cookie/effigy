# 02 016 Typed Runtime Container Error Foundation

Date: 2026-05-02
Roadmap: `g03.016`
Spec: `docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md`
Batch: `344`

## What Landed

- added the first typed runtime/container error family in `RunnerError`:
  - `ContainerRuntimePolicy`
  - `ContainerRuntimeExecNotReady`
- moved the first high-signal runtime-prep seams onto those variants:
  - container policy validation
  - compose backend validation
  - exec-readiness timeout after restart recovery
- kept the operator-facing error text stable while making the failure category
  explicit for tests and future routing
- added focused category-level tests in:
  - `src/runner/error/tests.rs`
  - `src/runner/container_runtime_prep.rs`

## Validation

- `cargo test -p effigy runner::error::tests:: --lib -- --nocapture`
- `cargo test -p effigy exec_readiness_recovery_ --lib -- --nocapture`
- `cargo test -p effigy validate_policy_runtime_uses_typed_policy_error_family --lib -- --nocapture`
- `cargo test -p effigy runtime_prep_ --lib -- --nocapture`
