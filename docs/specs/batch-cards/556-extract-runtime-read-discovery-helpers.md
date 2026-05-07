# 556 - Extract Runtime Read Discovery Helpers

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move runtime environment and repo discovery helpers out of
`crates/effigy-runtime/src/read.rs`.

## Scope

- create a focused read module for discovery helpers
- move:
  - running environment discovery
  - Effigy repo discovery under a scope root
  - discovery-only filesystem helpers if dependencies stay clean
- keep public runtime read functions stable through `read.rs`
- preserve status-all, stats-all, cache under-path, and data under-path
  behavior

## Non-Goals

- no status/logs/stats report schema changes
- no manager invocation changes
- no runtime behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when discovery helpers are out of `read.rs`, current
runtime callers still compile, and focused discovery/read checks pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-read-discovery-check2 cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-read-discovery-libcheck2 cargo check -p effigy --lib`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-read-discovery-test-b cargo test -p effigy-runtime discovery -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Extract runtime read report helpers.
