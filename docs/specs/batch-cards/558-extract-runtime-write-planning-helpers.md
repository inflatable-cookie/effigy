# 558 - Extract Runtime Write Planning Helpers

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move write-side lifecycle planning helpers out of
`crates/effigy-runtime/src/write.rs`.

## Scope

- create a focused write module for lifecycle operation plans
- move pure operation-plan helpers for down/reset/image cleanup
- keep public runtime write functions stable through `write.rs`
- preserve lifecycle/reset output and operation reports

## Non-Goals

- no lifecycle behavior changes
- no destructive-operation policy changes
- no manager invocation changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when write-side operation planning helpers are out of
`write.rs`, lifecycle callers still compile, and focused runtime checks pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-write-planning-check2 cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-write-planning-libcheck cargo check -p effigy --lib`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-write-planning-test-b cargo test -p effigy-runtime generated_service_image_refs -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Extract runtime write report helpers.
