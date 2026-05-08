# 551 - Extract Runtime Data Volume Planning Module

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Start splitting `crates/effigy-runtime/src/data.rs` by moving pure
volume/cache planning helpers into a focused data module.

## Scope

- create a `crates/effigy-runtime/src/data/` module tree if needed
- move pure helpers for generated volume/cache classification, reset planning,
  and report-adjacent data structures where dependencies stay clean
- keep public runtime data functions stable through the existing facade
- preserve `container data` and cache behavior

## Non-Goals

- no data command behavior changes
- no compose invocation changes
- no artifact/data pipeline changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when a first pure data-planning slice is out of
`data.rs`, the public data facade still compiles, and focused runtime checks
pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-data-planning-check cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-data-planning-libcheck cargo check -p effigy --lib`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-data-planning-test-a cargo test -p effigy-runtime global_cache_entries_mark_running_projects_in_use -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Extract runtime data volume IO helpers.
