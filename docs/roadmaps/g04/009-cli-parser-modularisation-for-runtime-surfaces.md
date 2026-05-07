# 009 - CLI Parser Modularisation For Runtime Surfaces

Generation: `g04`

Status: Active
Owner: Platform
Created: 2026-05-07
Depends on: [`008-manager-backed-runtime-read-write-shell.md`](./008-manager-backed-runtime-read-write-shell.md)

## Goal

Make CLI command parsing less centralised for high-churn runtime/container
features.

## Scope

- split `command_parsing.rs` by top-level command where still central
- ensure container/artifact/bootstrap parse modules have stable tests
- keep `effigy-cli` public `Command` enum stable unless a public break is
  selected
- add parse fixture tests for runtime/container/data surfaces

## Migration Targets

- `crates/effigy-cli/src/command_parsing.rs`
- `crates/effigy-cli/src/command_parsing_container.rs`
- `crates/effigy-cli/src/lib.rs`
- `src/tests/lib_tests_parse_tests/*`

## Acceptance Criteria

- no parser file over 700 lines
- all container data/artifact parse behavior has focused coverage
- CLI model remains the only public parsing contract

## Validation

- `cargo test -p effigy --lib parse_container`
- `cargo test -p effigy --lib parse_artifact`
- `cargo test -p effigy --lib parse_bootstrap`

## Next Task

Start card
[`562-scaffold-cli-parser-modularisation-lane.md`](../../specs/batch-cards/562-scaffold-cli-parser-modularisation-lane.md).
