# 567 - Extract Release Parser Module

Lane: [`051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md`](../051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move `effigy release ...` parsing out of
`crates/effigy-cli/src/command_parsing.rs`.

## Scope

- add `crates/effigy-cli/src/command_parsing_release.rs`
- move release status, gates, resume, simulate, verify-install, prepare, and
  execute parser helpers into that module
- add `crates/effigy-cli/src/command_parsing_changelog.rs` as the final small
  root-parser split needed to meet the line-count target
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

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-release-parser-check cargo check -p effigy-cli`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-release-parser-test cargo test -p effigy --lib parse_release -- --test-threads=1`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-changelog-parser-check cargo check -p effigy-cli`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-final-parser-test cargo test -p effigy --lib parse_ -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Card
[`568-scaffold-drift-guards-and-proof-matrix-lane.md`](./568-scaffold-drift-guards-and-proof-matrix-lane.md).
