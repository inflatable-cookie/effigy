# 524 - Split Rhai Feature Callback Host Modules

Lane: [`048-rhai-host-api-split-and-callback-purity-strict-lane.md`](../048-rhai-host-api-split-and-callback-purity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move callback-backed feature modules out of `host_api.rs` while leaving
runtime-sensitive `exec` and `container` cleanup for separate cards.

## Scope

- create focused callback host module files
- move non-container feature module builders:
  - `config`, `task`, `scan`, `docs`
  - `deploy`, `system`, `demo`, `changelog`, `cache`, `gateway`, `bundle`,
    `service`, `catalog`, `doctor`, `contracts`, `unlock`, `test`, `effigy`
- keep module names and callback feature payloads unchanged
- keep `exec` and `container` in `host_api.rs` for dedicated callback purity
  cards

## Non-Goals

- no `exec::run` migration
- no `container::*` migration
- no Rhai public API changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when non-container feature callback module builders no
longer live in `host_api.rs` and focused Rhai tests pass.

## Closeout

Non-container callback-backed Rhai feature modules now live in focused host
module files:

- `crates/effigy-rhai/src/host_api/feature_core.rs`
- `crates/effigy-rhai/src/host_api/feature_misc.rs`

`host_api.rs` retains only the registry shell plus `runtime`, `exec`, and
`container` builders for dedicated follow-up cards.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-feature-test cargo test -p effigy-rhai`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-feature-libcheck cargo check -p effigy --lib`

## Next Task

Start card
[`525-split-rhai-container-host-module.md`](./525-split-rhai-container-host-module.md).
