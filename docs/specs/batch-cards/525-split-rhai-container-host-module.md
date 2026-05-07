# 525 - Split Rhai Container Host Module

Lane: [`048-rhai-host-api-split-and-callback-purity-strict-lane.md`](../048-rhai-host-api-split-and-callback-purity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move the Rhai `container` module builder out of `host_api.rs` without changing
the public Rhai API.

## Scope

- create `crates/effigy-rhai/src/host_api/container.rs`
- move `build_container_module` and container-specific helper glue into the new
  module where ownership is clear
- keep current container callback payloads and function names stable
- keep the existing runtime activation backed `container::exec` behavior
- leave deeper container callback purity cleanup for the next card if the split
  exposes more direct-call drift

## Non-Goals

- no Rhai public API changes
- no container command behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the `container` module builder no longer lives in
`host_api.rs`, no Rhai host module file is over 500 lines, and focused Rhai
tests pass.

## Closeout

The Rhai `container` module builder now lives in
`crates/effigy-rhai/src/host_api/container.rs`. The registry shell is 373
lines, and all Rhai host module files are below 500 lines.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-container-test cargo test -p effigy-rhai`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-container-libcheck cargo check -p effigy --lib`

## Next Task

Start card
[`526-split-rhai-exec-host-module-and-review-callback-purity.md`](./526-split-rhai-exec-host-module-and-review-callback-purity.md).
