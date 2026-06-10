# 2026-05-05 - Rhai Runtime Context Helper

## Summary

Completed card `377`.

## Changed

- added `runtime` to the Rhai module registry
- exposed `runtime::context()`
- added active runtime-context handoff for Rhai execution
- wired runner Rhai execution to pass the active `EffigyRuntimeContext`
- added direct `effigy-rhai` runtime context coverage
- updated runner run-array Rhai coverage to assert command-root exposure

## Boundary

`exec::run(...)` is still planned. It should be implemented once
`TaskExecutionRequestBuilder` exists.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-rhai runtime -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy --lib run_manifest_task_run_array_rhai_steps_support_args_and_runtime_helpers -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next

Choose the next ready card.
