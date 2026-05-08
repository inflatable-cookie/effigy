# 566 - Extract Runtime Surface Parser Module

Lane: [`051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md`](../051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move runtime-adjacent built-in parsing out of
`crates/effigy-cli/src/command_parsing.rs`.

## Scope

- add `crates/effigy-cli/src/command_parsing_runtime.rs`
- move `exec`, `system`, `workspace`, `gateway`, and `service` parser helpers
  into that module
- preserve current flags, help routing, aliases, and parse errors
- run focused parse tests for the moved surfaces

## Non-Goals

- no public CLI behavior changes
- no command model changes
- no runtime/container execution changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the runtime-adjacent parser helpers are
module-owned, focused parse tests pass, and the remaining root parser line
count is rechecked against the `g04.009` target.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-runtime-parser-check cargo check -p effigy-cli`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-runtime-parser-test cargo test -p effigy --lib parse_ -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Card
[`567-extract-release-parser-module.md`](./567-extract-release-parser-module.md).
