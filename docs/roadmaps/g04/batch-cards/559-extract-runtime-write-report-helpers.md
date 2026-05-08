# 559 - Extract Runtime Write Report Helpers

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move write-side report rendering and annotation helpers out of
`crates/effigy-runtime/src/write.rs`.

## Scope

- create a focused write module for report helpers
- move:
  - down-all report rendering
  - shared-service annotation
  - gateway-route annotation
  - generic command report rendering
- keep public runtime write functions stable through `write.rs`
- preserve down/reset output text and JSON

## Non-Goals

- no report schema changes
- no lifecycle behavior changes
- no manager invocation changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when write report helpers are out of `write.rs`,
write-side tests still pass, and focused runtime checks pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-write-report-check cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-write-report-libcheck cargo check -p effigy --lib`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-write-report-test-a cargo test -p effigy-runtime container_down_all_report_renders_text_and_json -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Extract runtime shell exec argument helpers.
