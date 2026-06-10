# Effigy CLI Foundation Extraction

Date: 2026-04-15
Roadmap: `g02.010`
Card: `144`

## Summary

The CLI command model and parse grammar now live in a real workspace crate.

This removes the top-level command/args/enums from `src/lib.rs` and removes
the command parser implementation from `src/cli/parse/command_parsing.rs`.

## What Changed

- added `crates/effigy-cli`
- moved the command model into `crates/effigy-cli/src/lib.rs`
- moved the global JSON and command parser modules into `crates/effigy-cli/src/`
- reduced `src/lib.rs` to a thin reexport surface for the command model
- reduced `src/cli/parse/mod.rs` to a thin compatibility wrapper

## Boundary Result

The CLI shell is no longer the next ambiguous seam.

The remaining largest shell-facing surface is now the TUI/browser runtime
stack, especially `src/tui/demo_browser.rs` and the wider `src/tui/`
subtree.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: `CLI command model and parse grammar inline in src` -> `CLI command model and parse grammar owned by crates/effigy-cli`
- Remaining gap: `TUI/browser runtime extraction before release closure can resume honestly`

## Validation Performed

- command: `cargo fmt --all`
  - result: passed
- command: `cargo test -p effigy-cli`
  - result: passed
- command: `cargo test help_and_flag_tests --lib`
  - result: passed
- command: `cargo test --test cli_output_tests help_and_flags_tests`
  - result: passed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Next Task

Execute [`145-implement-effigy-tui-foundation-extraction.md`](../../../specs/batch-cards/145-implement-effigy-tui-foundation-extraction.md).
