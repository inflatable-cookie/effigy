# 144 Implement Effigy CLI Foundation Extraction

Status: archived
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the CLI command model and parse grammar into a real workspace crate so
the root crate stops owning that shell contract inline.

## In Scope

- add a workspace crate for the CLI command model and parse grammar
- move the top-level command/args/enums out of `src/lib.rs`
- move global JSON parsing and command parsing out of `src/cli/parse/`
- reconnect the root crate through compatibility reexports/adapters

## Out Of Scope

- TUI/browser extraction
- help rendering extraction
- release-lane execution

## Acceptance Criteria

- the CLI command model no longer lives inline in `src/lib.rs`
- the parse grammar no longer lives in `src/cli/parse/command_parsing.rs`
- the next remaining shell seam is explicit

## Validation

- `cargo fmt --all`
- `cargo test -p effigy-cli`
- `cargo test help_and_flag_tests --lib`
- `cargo test --test cli_output_tests help_and_flags_tests`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute [`145-implement-effigy-tui-foundation-extraction.md`](./145-implement-effigy-tui-foundation-extraction.md).
