# 554 - Extract Runtime Data Transfer Validation

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-07

## Goal

Continue reducing `crates/effigy-runtime/src/data.rs` by moving transfer
target resolution and validation helpers into a focused module.

## Scope

- create `crates/effigy-runtime/src/data/transfer.rs`
- move:
  - managed volume lookup for transfer targets
  - export/import path validation
  - generated-compose data path validation if dependencies stay clean
- keep public runtime data functions stable through `data.rs`
- preserve transfer error text

## Non-Goals

- no data export/import behavior changes
- no report schema changes
- no runtime command invocation changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when transfer validation helpers are out of `data.rs`,
public runtime data callers still compile, and focused runtime checks pass.

## Validation

- `cargo check -p effigy-runtime`
- `cargo check -p effigy --lib`
- `git diff --check`

## Next Task

Extract runtime data transfer validation helpers.
