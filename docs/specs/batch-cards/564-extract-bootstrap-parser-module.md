# 564 - Extract Bootstrap Parser Module

Lane: [`051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md`](../051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move `effigy bootstrap` parsing out of
`crates/effigy-cli/src/command_parsing.rs`.

## Scope

- add `crates/effigy-cli/src/command_parsing_bootstrap.rs`
- move bootstrap clone, teardown, deps, and children parser helpers into that
  module
- keep `parse_bootstrap_db_seed` available to container data seed parsing
- preserve current bootstrap flags, aliases, and parse errors
- run focused bootstrap parse tests

## Non-Goals

- no public CLI behavior changes
- no bootstrap command model changes
- no container data parser rewrite beyond the shared DB seed helper import
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when bootstrap parser ownership is out of the root parser,
bootstrap parse tests pass, and container data seed parsing still reuses the
same DB seed parser.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-bootstrap-parser-check2 cargo check -p effigy-cli`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-bootstrap-parser-test2 cargo test -p effigy --lib parse_bootstrap -- --test-threads=1`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-container-after-bootstrap-test2 cargo test -p effigy --lib parse_container -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Card
[`565-extract-container-data-parser-module.md`](./565-extract-container-data-parser-module.md).
