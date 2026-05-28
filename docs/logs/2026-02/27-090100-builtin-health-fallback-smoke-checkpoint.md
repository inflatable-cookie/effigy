# 2026-02-27 Built-in Health Fallback Smoke Checkpoint

## Scope

Validate new built-in `health` fallback behavior and prefixed routing in a real workspace.

## Environment

- Workspace: `~/Dev/projects/example-app`
- Effigy invocation: `cargo run --manifest-path ~/Dev/projects/effigy/Cargo.toml --bin effigy -- ...`
- Date: 2026-02-27

## Commands and Outcomes

1. `effigy health`
- Status: Pass
- Result: runs pulse-style report at workspace root.
- Result: reports missing root `tasks.health` in `example-app/effigy.toml` with action guidance.

2. `effigy farmyard/health`
- Status: Pass
- Result: routes pulse-style report to prefixed catalog root (`farmyard`) only.
- Result: reports missing root `tasks.health` in `farmyard/effigy.toml` with action guidance.

3. `effigy tasks --task health`
- Status: Pass
- Result: built-in task match includes `health` alias description.

## Summary

Built-in `health` fallback is functional for both unprefixed and prefixed catalog requests.  
Task discovery also surfaces the `health` built-in clearly when filtering.

## Follow-ups

- Optional: add explicit `tasks.health` tasks in `example-app` root and `farmyard` to clear pulse risk output and codify team-owned health checks.
