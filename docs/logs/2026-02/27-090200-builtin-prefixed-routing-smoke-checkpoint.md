# 2026-02-27 Built-in Prefixed Routing Smoke Checkpoint

## Scope

Validate foundational built-in routing behavior in a real workspace (`acowtancy`) after generic built-in dispatch changes.

## Environment

- Workspace: `/Users/betterthanclay/Dev/projects/acowtancy`
- Effigy binary (dev): `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- ...`
- Date: 2026-02-27

## Commands and Outcomes

1. `effigy help`
- Status: Pass
- Result: General help rendered with built-in command list including `help`, `test`, and `tasks`.

2. `effigy farmyard/tasks`
- Status: Pass
- Result: Task catalog scoped to `farmyard` only (`catalogs: 1`), with farmyard task list shown.
- Result: Built-in tasks table still rendered in scoped context.

3. `effigy farmyard/test --plan`
- Status: Pass
- Result: Built-in test detection executed in `farmyard` scope (`targets: 1`, `Target: farmyard`).
- Result: Runner selected as `cargo-nextest` with evidence and fallback chain rendered.

## Summary

Built-in routing now behaves like catalog task routing at the foundational level for prefixed invocations. `catalog/builtin` requests correctly resolve target root scope without per-task special-case behavior in user workflows.

## Follow-ups

- Optional: add explicit smoke for `effigy farmyard/repo-pulse` in this workspace to close the full built-in matrix.
