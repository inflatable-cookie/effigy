# 521 - Split Rhai Pure Utility Host Modules

Lane: [`048-rhai-host-api-split-and-callback-purity-strict-lane.md`](../048-rhai-host-api-split-and-callback-purity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make the first Rhai host split by moving pure utility module builders out of
`host_api.rs`.

## Scope

- create a focused internal Rhai utility host module file
- move `time`, `path`, `json`, `toml`, `str`, and `random` module builders
- keep public Rhai module names unchanged
- keep `register_host_api` as the registration shell
- avoid callback behavior changes

## Non-Goals

- no `exec`, `container`, `task`, or runtime-sensitive callback migration
- no Rhai public API changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when pure utility module builders no longer live in
`host_api.rs`, with existing Rhai tests still passing.

## Closeout

Added `crates/effigy-rhai/src/host_api/utility.rs` and moved the pure utility
module builders for `time`, `path`, `json`, `toml`, `str`, and `random` out of
`host_api.rs`. Registration stays centralized through `register_host_api`, but
the utility module now owns its own static-module registration.

`host_api.rs` dropped from 2164 lines to 1983 lines.

## Validation

- `cargo test -p effigy-rhai` passed
- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-utility-check cargo check -p effigy --lib`
  passed with the existing `runtime_activation_report_for_result` dead-code
  warning
- `git diff --check` passed

## Next Task

Start card
[`522-split-rhai-filesystem-host-module.md`](./522-split-rhai-filesystem-host-module.md).
