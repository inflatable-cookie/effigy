# 588 - Extract Effigy Data Test Module And Close Decomposition

Lane: [`059-planning-crate-decomposition-strict-lane.md`](../059-planning-crate-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Reduce `effigy-data` source-file pressure without changing behavior.

## Scope

- move the large inline `effigy-data` test module to `tests.rs`
- keep public exports stable
- avoid a broad behavior-free source split after the seed/dump migrations
  already changed this crate in the same batch
- close `g04.017`

## Non-Goals

- no public API changes
- no further seed/dump behavior edits
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the data crate test module is out of `lib.rs` and
the crate still passes focused tests.

## Validation

- `cargo test -p effigy-data`
- `git diff --check`

## Next Task

Planning stop or human-selected next `g04` roadmap.
