# 523 - Split Rhai Process Http Search Host Modules

Lane: [`048-rhai-host-api-split-and-callback-purity-strict-lane.md`](../048-rhai-host-api-split-and-callback-purity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move remaining non-callback host utility modules out of `host_api.rs`.

## Scope

- create focused host module files for process, HTTP, and search helpers
- move `process`, `http`, and `search` module builders
- keep process cwd/env/stdin/stdout behavior unchanged
- keep HTTP request/download behavior unchanged
- keep search behavior unchanged
- leave `exec`, `container`, `task`, and feature callback modules in place

## Non-Goals

- no `exec::run` migration
- no container callback migration
- no public Rhai API changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when process, HTTP, and search helper registrations no
longer live in `host_api.rs` and focused Rhai tests pass.

## Closeout

Added focused host module files for `process`, `http`, and `search`. The
process module owns process run/stream/tee registrations, the HTTP module owns
request/download registrations and download option parsing, and the search
module owns file search registration.

`host_api.rs` dropped from 1524 lines to 1238 lines.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-process-test cargo test -p effigy-rhai`
  passed
- `CARGO_TARGET_DIR=/tmp/effigy-g04-rhai-process-check cargo check -p effigy --lib`
  passed with the existing `runtime_activation_report_for_result` dead-code
  warning
- `git diff --check` passed

## Next Task

Start card
[`524-split-rhai-feature-callback-host-modules.md`](./524-split-rhai-feature-callback-host-modules.md).
