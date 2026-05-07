# 553 - Extract Runtime Data Report Helpers

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move report rendering and annotation helpers out of
`crates/effigy-runtime/src/data.rs`.

## Scope

- create `crates/effigy-runtime/src/data/report.rs`
- move:
  - text/JSON report rendering helper
  - gateway route annotation helper
  - shared service annotation helper
  - warning annotation helper
- keep `RegisteredGatewayRoute` available to public runtime data callers
- preserve report JSON/text shape

## Non-Goals

- no report schema changes
- no data command behavior changes
- no runtime command invocation changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when report helpers are out of `data.rs`, public runtime
data callers still compile, and focused runtime checks pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-data-report-check cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-data-report-libcheck cargo check -p effigy --lib`
- PASS: `git diff --check`

## Next Task

Extract runtime data transfer validation helpers.
