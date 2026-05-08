# 563 - Extract Artifact Parser Module

Lane: [`051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md`](../051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move `effigy artifact` parsing out of
`crates/effigy-cli/src/command_parsing.rs`.

## Scope

- add `crates/effigy-cli/src/command_parsing_artifact.rs`
- move artifact inspect/stage/capture parser helpers into that module
- keep `artifact` and `artefact` aliases unchanged
- preserve current artifact parse errors and flags
- run focused artifact parse tests

## Non-Goals

- no public CLI behavior changes
- no artifact command model changes
- no container parser changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when artifact parser ownership is out of the root parser,
artifact parse tests pass, and the root parser line count is reduced.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-artifact-parser-check2 cargo check -p effigy-cli`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-cli-artifact-parser-test2 cargo test -p effigy --lib parse_artifact -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Card
[`564-extract-bootstrap-parser-module.md`](./564-extract-bootstrap-parser-module.md).
