# 377 - Expose Rhai Runtime Context Helper

Lane: [`036-universal-runtime-context-and-path-authority-strict-lane.md`](../036-universal-runtime-context-and-path-authority-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

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

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-rhai runtime -- --nocapture`
- targeted runner Rhai script test
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next Task

Implement this card.
