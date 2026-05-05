# 2026-05-05 - Rhai Exec Run Helper

## Summary

Completed card `379`.

## Changed

- added `effigy-execution` as an `effigy-rhai` dependency
- added `exec` to the Rhai module registry
- exposed capture-mode `exec::run(command, options)`
- routed host/container/local-handoff plans through `TaskExecutionRequestBuilder`
- returned process-like output plus route detail
- added direct Rhai and runner run-array coverage

## Boundary

This is not full runner execution migration. It is the Rhai command helper
needed to express the DecodeLabs mysql seed shape without local branching.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-rhai exec -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy --lib run_manifest_task_run_array_rhai_steps_support_args_and_runtime_helpers -- --nocapture`

## Next

Create the next execution request migration card.
