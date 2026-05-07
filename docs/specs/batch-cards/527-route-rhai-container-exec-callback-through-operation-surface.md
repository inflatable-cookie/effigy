# 527 - Route Rhai Container Exec Callback Through Operation Surface

Lane: [`048-rhai-host-api-split-and-callback-purity-strict-lane.md`](../048-rhai-host-api-split-and-callback-purity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Remove the remaining Rhai container-sensitive callback drift by routing
`container::exec` through the typed container operation path instead of direct
runner helper calls.

## Scope

- update `src/runner/script_command/mod.rs` Rhai `container_exec*` callbacks
- preserve current Rhai `container::exec` public behavior and return map shape
- keep runtime activation before container exec
- use or introduce a narrow operation-surface adapter rather than calling
  `run_container_exec_capture*` directly from the Rhai callback
- keep stdin-file behavior for DecodeLabs mysql seed scripts
- add or preserve drift scan coverage for `run_container_exec_capture` in Rhai
  callback paths

## Non-Goals

- no Rhai public API changes
- no broad container lifecycle refactor
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when Rhai container exec callbacks no longer call
`run_container_exec_capture*` directly, focused Rhai tests pass, and the
DecodeLabs mysql seed proof remains green.

## Closeout

Rhai `container_exec*` callbacks now create `ContainerCapturedExecOperation`
values and call the narrower operation adapter
`run_container_exec_operation_capture`. The old `run_container_exec_capture*`
helpers remain for non-Rhai callers, but the Rhai callback path no longer calls
them directly.

The DecodeLabs mysql seed proof remains covered by the `effigy-rhai` focused
test suite.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-callback-test cargo test -p effigy-rhai`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-callback-libcheck cargo check -p effigy --lib`
- `! rg -n 'run_container_exec_capture' crates/effigy-rhai/src src/runner/script_command/mod.rs`
- `git diff --check`

## Next Task

Start card
[`528-close-rhai-host-api-split-and-callback-purity.md`](./528-close-rhai-host-api-split-and-callback-purity.md).
