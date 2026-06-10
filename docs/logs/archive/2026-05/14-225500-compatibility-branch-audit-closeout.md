# Compatibility Branch Audit Closeout

Date: 2026-05-14
Roadmap: `g06.007`
Batch card: `807`

## Summary

Deleted two compatibility-only paths that no longer had current docs,
contracts, or released-surface proof behind them, and recorded the branches
that still remain justified.

## Changes

- removed stale `catalogue` host-native routing from
  [`crates/effigy-exec/src/routing.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-exec/src/routing.rs)
- removed the flat `docs check-*` parser shims and migration-error helper from
  [`crates/effigy-cli/src/command_parsing_docs.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-cli/src/command_parsing_docs.rs)
- updated the parser proof in
  [`src/tests/lib_tests_parse_tests/docs_and_contracts_option_tests.rs`](/Users/tom/Dev/projects/effigy/src/tests/lib_tests_parse_tests/docs_and_contracts_option_tests.rs)
  so the retired flat spelling now fails as an ordinary unknown argument

## Retained Compatibility

Kept these branches because active proof still depends on them:

- `release resume`
- `--dry-run`
- `--allow-stale`
- migration-sensitive runtime and gateway behavior without separate dead-branch
  proof

## Outcome

- deleted internal-only compatibility debt without changing the released
  surface
- reduced parser and routing special cases
- left the remaining compatibility debt explicit instead of silently carrying
  it forward

## Vision Target Delta

- primary tags: `ROUTE`, `CONTRACT`, `MAINT`
- moved: compatibility branch set went from unclassified legacy/shim debt to a
  smaller proved-live set with two dead paths deleted
- remains open: `g06.008` runner-private domain logic reduction and final
  `g06.001` closeout proof

## Validation

- `cargo fmt --all`
- `cargo test docs_and_contracts_option_tests`
- `cargo test help_and_flag_tests`
- `cargo run --bin effigy -- qa:released-surface --repo .`
