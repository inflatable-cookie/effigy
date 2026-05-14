# CLI Help Topic Descriptor Convergence

Date: 2026-05-14

## Summary

Completed card `732`, the CLI help topic descriptor convergence slice.

## Changes

- added `crates/effigy-cli/src/help/registry.rs`
- moved builtin help-topic lookup onto the shared descriptor surface
- moved the general-help topic inventory onto the same descriptor surface
- routed help rendering dispatch through the registry while keeping topic body
  text explicit in the current topic modules
- advanced current ready work to card `733`

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Baseline: builtin help topic lookup and general-help command inventory were
  maintained in separate manual registries.
- Current state: CLI help topic lookup, general-help builtin inventory, and
  rendering dispatch now share one descriptor surface.
- Remaining open: area-local fixture dedup, docs reference refresh, and final
  closeout.

## Validation

- `cargo test -p effigy help_dispatch`
- `cargo test -p effigy cli_output_tests`
- `cargo fmt --all -- --check`
- `git diff --check`

## Validation Blockers

- `cargo test -p effigy-cli` still fails on the unrelated header-width unit test
  `header::tests::render_cli_header_width_grows_to_fit_long_version`.

## Next Task

Execute `733` to add area-local test builders for the highest-duplication seams.
