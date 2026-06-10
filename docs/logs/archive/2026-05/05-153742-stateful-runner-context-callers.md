# Stateful Runner Context Callers

Date: 2026-05-05

## Change

Completed card `389`.

Migrated stateful command entry modules to context-backed helpers.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- focused `container_command`, `exec_command`, and `defer_command` test filters
- exact CLI release deferral regression test
- narrow workspace context tests

## Next Task

Implement card `390`.
