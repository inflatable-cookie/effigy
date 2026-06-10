# Release Model Owner Extraction

Date: 2026-05-14
Roadmap: `g06.003`
Batch card: `803`

## Summary

Pulled the core release data model out of
`crates/effigy-release/src/lib.rs` into a dedicated owner module.

## Changes

- added [`crates/effigy-release/src/model.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-release/src/model.rs)
- moved the main release state, gate, prepare, execute, verify, and error
  model types into the new module
- rewired [`crates/effigy-release/src/lib.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-release/src/lib.rs)
  to re-export the public model surface

## Outcome

- `crates/effigy-release/src/lib.rs` dropped from `1622` lines to `1314`
- `effigy scan god-files --json` no longer reports `effigy-release/src/lib.rs`
- only `src/runner/state_command.rs` remains as a warning-level god file

## Validation

- `cargo test release`
- `cargo test --test cli_output_tests cli_release`
- `cargo run --bin effigy -- scan god-files --json`
