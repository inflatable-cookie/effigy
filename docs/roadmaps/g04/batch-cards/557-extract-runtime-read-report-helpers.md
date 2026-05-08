# 557 - Extract Runtime Read Report Helpers

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move read-only container report shaping helpers out of
`crates/effigy-runtime/src/read.rs`.

## Scope

- create a focused read module for report helpers
- move:
  - report rendering helper
  - warning annotation helper
  - status-all entry shaping if dependencies stay clean
- keep public runtime read functions stable through `read.rs`
- preserve status/logs/stats output text and JSON

## Non-Goals

- no report schema changes
- no status/logs/stats command behavior changes
- no manager invocation changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when read report helpers are out of `read.rs`, the read
module stays focused on command orchestration, and focused runtime checks pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-read-report-check cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-read-report-libcheck2 cargo check -p effigy --lib`
- PASS: `git diff --check`

## Next Task

Extract runtime write planning helpers.
