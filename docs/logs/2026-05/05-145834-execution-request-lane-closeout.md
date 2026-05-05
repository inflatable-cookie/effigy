# 2026-05-05 - Execution Request Lane Closeout

## Summary

Closed `g03.032` and strict lane `037`.

## Changed

- migrated remaining runner task callers to request-building helpers
- migrated bootstrap seed task handoff to `ExecutionSurface::Bootstrap`
- migrated demo task execution to `ExecutionSurface::Demo`
- migrated Rhai task callbacks to `ExecutionSurface::Rhai`
- migrated doctor and built-in ports to request-backed task execution
- migrated first-party DecodeLabs and Underlay service-local Rhai scripts from
  `container::exec(...)` to `exec::run(...)`

## Boundary

The old `entry` functions remain as the compatibility bridge behind
`run_manifest_task_request(...)`. Full deletion waits for deeper pipeline
refactoring outside `g03.032`.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy --lib direct_task_dispatch_runs_through_execution_request_boundary -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy --lib run_manifest_task_run_array_task_reference_output_contract_table -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-rhai exec -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-rhai first_party_rhai -- --nocapture`

## Next

Continue with `g03.031` container manager facade or open the next queued
roadmap deliberately.
