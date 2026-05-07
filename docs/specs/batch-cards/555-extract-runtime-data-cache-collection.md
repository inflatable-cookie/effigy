# 555 - Extract Runtime Data Cache Collection

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move profile-wide cache collection helpers out of
`crates/effigy-runtime/src/data.rs`.

## Scope

- create `crates/effigy-runtime/src/data/cache.rs`
- move:
  - profile-wide cache entry collection
  - cache volume metadata lookup orchestration
  - project running-state helper if dependencies stay clean
- keep public runtime data functions stable through `data.rs`
- preserve cache list/prune behavior

## Non-Goals

- no cache report schema changes
- no runtime volume command behavior changes
- no manager invocation changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when profile-wide cache collection helpers are out of
`data.rs`, public runtime data callers still compile, and focused runtime
checks pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-data-cache-check cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-data-cache-libcheck cargo check -p effigy --lib`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-data-cache-test-a cargo test -p effigy-runtime global_cache_entries_mark_running_projects_in_use -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Extract runtime read discovery helpers.
