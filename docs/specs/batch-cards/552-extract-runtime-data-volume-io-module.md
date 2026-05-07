# 552 - Extract Runtime Data Volume IO Module

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Continue splitting `crates/effigy-runtime/src/data.rs` by moving runtime volume
hydration, metadata inspection, and transfer command helpers into a focused
data IO module.

## Scope

- create `crates/effigy-runtime/src/data/volume_io.rs`
- move side-effect adapter helpers that operate through the injected
  `run_runtime_volume_capture` callback:
  - managed volume hydration
  - runtime volume metadata inspection
  - export/import command execution wrapper
- keep public runtime data functions stable through `data.rs`
- preserve report rendering and validation behavior

## Non-Goals

- no runtime command invocation redesign
- no data export/import behavior changes
- no artifact/data pipeline changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when volume IO helpers are out of `data.rs`, public
runtime data callers still compile, and focused runtime checks pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-data-volume-io-check cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-data-volume-io-libcheck cargo check -p effigy --lib`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-data-volume-io-test-a cargo test -p effigy-runtime global_cache_entries_mark_running_projects_in_use -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Extract runtime data report annotation helpers.
