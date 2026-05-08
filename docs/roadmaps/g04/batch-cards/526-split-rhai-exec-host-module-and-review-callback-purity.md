# 526 - Split Rhai Exec Host Module And Review Callback Purity

Lane: [`048-rhai-host-api-split-and-callback-purity-strict-lane.md`](../048-rhai-host-api-split-and-callback-purity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move the Rhai `exec` module builder out of `host_api.rs` and verify the
runtime-sensitive Rhai callback path is still routed through the task execution
request model.

## Scope

- create `crates/effigy-rhai/src/host_api/exec.rs`
- move `build_exec_module` and its private execution-plan helpers into the new
  module where ownership is clear
- keep `exec::run` and `exec::plan` public behavior unchanged
- confirm `exec::run(... run_in = "container" ...)` still uses
  `TaskExecutionRequestBuilder`
- inventory any remaining Rhai container-sensitive direct callback drift

## Non-Goals

- no Rhai public API changes
- no task execution behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the `exec` module builder no longer lives in
`host_api.rs`, `exec::run` remains builder-backed, and focused Rhai tests pass.

## Closeout

The Rhai `exec` module builder now lives in
`crates/effigy-rhai/src/host_api/exec.rs`. `exec::run` still resolves through
`TaskExecutionRequestBuilder`, including the `run_in = "container"` route.

Callback-purity review found one remaining gap: `container::exec` is split into
its own module, but its runner callback still delegates to
`run_container_exec_capture*` in `src/runner/script_command/mod.rs`. That needs
a focused follow-up so the container-sensitive Rhai route lands on typed
container operation/request surfaces instead of runner helper calls.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-exec-test cargo test -p effigy-rhai`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-exec-libcheck cargo check -p effigy --lib`

## Next Task

Start card
[`527-route-rhai-container-exec-callback-through-operation-surface.md`](./527-route-rhai-container-exec-callback-through-operation-surface.md).
