# 2026-04-16 237500 - Effigy Contracts Foundation Extraction

## Summary

Completed `219` by extracting the first contracts-domain slice into a new
workspace crate.

## What Changed

- added `crates/effigy-contracts`
- moved selection-validation, schema-index loading, selection shaping, and
  selected-schema check orchestration into the new crate
- rewired `src/runner/contracts_command.rs` into a thin runner adapter over the
  extracted contracts APIs
- reduced `src/runner/contracts_command.rs` from `926` lines to `157`

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`220-decide-post-contracts-foundation-extraction-boundary.md`](../../specs/batch-cards/220-decide-post-contracts-foundation-extraction-boundary.md)
to decide whether the contracts seam can pause or still needs one more bounded
follow-up.
