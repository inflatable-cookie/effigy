# 587 - Split Effigy Container Ops Module Owners

Lane: [`059-planning-crate-decomposition-strict-lane.md`](../059-planning-crate-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Split `effigy-container-ops` into operation-owned modules.

## Scope

- move lifecycle operation types into `lifecycle.rs`
- move read operation types into `read.rs`
- move exec/shell operation types into `exec.rs`
- move data/cache/volume operation types into domain modules
- move side-effect, confirmation, request, plan, and report ownership into
  focused modules where useful
- keep public exports stable from `lib.rs`

## Non-Goals

- no behavior changes
- no public API removals
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `crates/effigy-container-ops/src/lib.rs` is mostly
exports and tests still pass.

## Validation

- `cargo test -p effigy-container-ops`
- `git diff --check`

## Next Task

Start
[`588-extract-effigy-data-test-module-and-close-decomposition.md`](./588-extract-effigy-data-test-module-and-close-decomposition.md).
