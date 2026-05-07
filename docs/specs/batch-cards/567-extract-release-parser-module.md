# 567 - Extract Release Parser Module

Lane: [`051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md`](../051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-07

## Goal

Move `effigy release ...` parsing out of
`crates/effigy-cli/src/command_parsing.rs`.

## Scope

- add `crates/effigy-cli/src/command_parsing_release.rs`
- move release status, gates, resume, simulate, verify-install, prepare, and
  execute parser helpers into that module
- preserve current release flags, dry-run aliases, gate-check flags, and parse
  errors
- run focused release parse tests

## Non-Goals

- no release execution
- no release command behavior changes
- no changelog parser split unless needed after this card
- no `.github/workflows/` edits

## Exit Condition

This card is complete when release parsing is module-owned, release parse tests
pass, and the root parser line count is rechecked against the `g04.009`
target.

## Validation

- `cargo check -p effigy-cli`
- `cargo test -p effigy --lib parse_release`
- `git diff --check`

## Next Task

Extract release parser module.
