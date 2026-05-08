# 565 - Extract Container Data Parser Module

Lane: [`051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md`](../051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move `effigy container data ...` parsing out of
`crates/effigy-cli/src/command_parsing_container.rs`.

## Scope

- add `crates/effigy-cli/src/command_parsing_container_data.rs`
- move `data list/export/import/pull-production/seed/dump` parser helpers into
  that module
- keep shared bootstrap DB seed parsing for `container data seed`
- preserve current data flags, aliases, positional forms, and parse errors
- run focused container parse tests

## Non-Goals

- no public CLI behavior changes
- no container data command model changes
- no data pipeline/runtime changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when container data parsing is module-owned,
`command_parsing_container.rs` is below the roadmap threshold or has a clear
remaining split documented, and container parse tests pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-container-data-parser-check cargo check -p effigy-cli`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-container-data-parser-test cargo test -p effigy --lib parse_container -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Card
[`566-extract-runtime-surface-parser-module.md`](./566-extract-runtime-surface-parser-module.md).
