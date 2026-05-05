# 2026-05-05 - Embedded Task Execution Request Migration

## Summary

Completed card `381`.

## Changed

- migrated `run_embedded_task(...)` to `TaskExecutionRequestBuilder`
- tagged embedded task requests as `ExecutionSurface::RunArray`
- preserved existing execution pipeline behavior

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy --lib run_manifest_task_run_array_task_reference_output_contract_table -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next

Choose the next execution request migration card.
