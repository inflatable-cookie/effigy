# Command Surface Descriptor Seam

Date: 2026-06-04
Roadmap: `g08.009`
Batch card: `1038`

## Summary

Added a small command-surface descriptor seam in `effigy-cli` and migrated the
help registry metadata consumer to it.

Parser and runner dispatch remain explicit. No CLI grammar, JSON contract, or
help text behavior was changed.

## Changes

- Added `crates/effigy-cli/src/command_surface.rs` as the reviewable owner for
  help topic command names, general help rows, descriptions, and deferred
  built-in hiding metadata.
- Updated `crates/effigy-cli/src/help/registry.rs` so render wiring uses the
  shared command descriptors instead of carrying duplicate metadata fields.
- Added descriptor coverage tests for current help topics, direct `--help`
  routes, task-style built-in help routes, and general help row metadata.

## Deferred Cleanup

- Parser arms remain manual by design for this card.
- Runner dispatch remains manual by design for this card.
- Shell completion still has a separate built-in command index under
  `crates/effigy-builtin/src/completion/scripts/command_index.rs`. It mixes
  `effigy-builtin` task registry data with CLI command names, so it should be a
  later cross-crate descriptor consumer rather than part of this first seam.

## Validation

- `cargo fmt --all`: pass
- `cargo test -p effigy-cli`: pass
- `cargo test --test cli_output_tests released_surface_ -- --nocapture`: pass

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline -> current: command/help metadata now has one descriptor source for
  the help registry and coverage tests catch missing descriptor metadata for
  current help routes.
- Open: migrate the next suitable command metadata consumer later; continue now
  to Rhai feature descriptors.

## Next Task

Run `1039`.
