# 522 - Split Rhai Filesystem Host Module

Lane: [`048-rhai-host-api-split-and-callback-purity-strict-lane.md`](../048-rhai-host-api-split-and-callback-purity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move filesystem-oriented Rhai host helpers out of `host_api.rs`.

## Scope

- create a focused filesystem host module file
- move `fs` module builder and direct filesystem helper registrations
- keep path resolution relative to `ScriptContext.cwd`
- preserve existing `fs::*` public Rhai functions and error text
- keep callback-sensitive modules untouched

## Non-Goals

- no `process`, `exec`, `container`, or task callback migration
- no Rhai public API changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when filesystem helper registrations no longer live in
`host_api.rs` and focused Rhai tests pass.

## Closeout

Added `crates/effigy-rhai/src/host_api/fs.rs` and moved the `fs` module builder,
direct filesystem helpers, env-file helpers, and dotenv entry mutation helpers
out of `host_api.rs`.

`host_api.rs` dropped from 1983 lines to 1524 lines. The new filesystem module
is 474 lines, under the lane threshold.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-fs-test cargo test -p effigy-rhai`
  passed
- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-fs-check cargo check -p effigy --lib`
  passed with the existing `runtime_activation_report_for_result` dead-code
  warning
- `git diff --check` passed

## Next Task

Start card
[`523-split-rhai-process-http-search-host-modules.md`](./523-split-rhai-process-http-search-host-modules.md).
