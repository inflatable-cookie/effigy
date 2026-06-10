# Manifest Semantic Owner Split

Date: 2026-05-19  
Roadmap: [`g07.065`](../../../roadmaps/g07/065-manifest-semantic-owner-split.md)  
Batch card: [`1015`](../../../roadmaps/g07/batch-cards/1015-split-manifest-semantic-ownership.md)  
Strict lane: [`095`](../../../specs/095-residual-maintainability-follow-through-strict-lane.md)

## What Changed

- replaced the old monolithic
  `crates/effigy-codegraph/src/language/manifest/semantic.rs`
  with owned semantic modules under
  `crates/effigy-codegraph/src/language/manifest/semantic/`
- split ownership into:
  - `mod.rs` for orchestration
  - `typed.rs` for typed manifest sections
  - `raw.rs` for raw config-table sections
  - `support.rs` for shared symbol, edge, and run helpers

## Result

- the manifest semantic god-file finding is gone
- `effigy scan god-files --json` now reports `2` warning-only findings instead
  of `3`
- remaining warning-only files:
  - `crates/effigy-codegraph/src/tests.rs`
  - `src/runner/script_command/mod.rs`

## Validation

- `cargo fmt --all -- --check`
- `cargo test -p effigy-codegraph --quiet`
- `effigy scan god-files --json`

## Vision Target Delta

- primary vision tags touched: `MAINT`
- moved:
  - manifest semantic graph ownership is now divided by real concern instead of
    one 2.5k-line mixed owner
  - the reopened `g07` warning-only god-file set dropped from `3` to `2`
- remains open:
  - `1016` through `1021`

## Next Task

Execute `1016`.
