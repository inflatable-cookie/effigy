# 377 - Expose Rhai Runtime Context Helper

Lane: [`036-universal-runtime-context-and-path-authority-strict-lane.md`](../036-universal-runtime-context-and-path-authority-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Expose the captured `EffigyRuntimeContext` to Rhai scripts as a read-only
runtime context map.

## Scope

- add runtime context data to `effigy-rhai::ScriptContext`
- expose `runtime::context()`
- include invocation cwd, command root, repo override, invocation mode,
  inside-container handoff, and host facts
- update script execution call sites to pass the captured context where
  available
- keep `exec::run(...)` for the later execution-builder card

## Exit Condition

This card is complete when Rhai scripts can call `runtime::context()` and see
the same boot-time context the runner captured.

## Closeout

Rhai now exposes `runtime::context()` through `effigy-rhai`. Runner script
execution passes the active `EffigyRuntimeContext` when available, with a lossy
repo-root fallback for direct Rhai crate execution.

The context map includes:

- `invocation_cwd`
- `command_root`
- `repo_override`
- `invocation_mode`
- `inside_container_handoff`
- `host`

`exec::run(...)` remains the next implementation step because it depends on the
canonical execution request builder.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-rhai runtime -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy --lib run_manifest_task_run_array_rhai_steps_support_args_and_runtime_helpers -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next Task

Choose the next ready card.
