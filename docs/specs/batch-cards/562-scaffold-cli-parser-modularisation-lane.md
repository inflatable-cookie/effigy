# 562 - Scaffold CLI Parser Modularisation Lane

Lane: [`051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md`](../051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Inventory the current parser hotspots and select the first bounded `g04.009`
implementation slice.

## Scope

- measure current parser file sizes
- inventory parse tests for container, artifact, and bootstrap surfaces
- identify the safest first parser split
- update the lane with the chosen first implementation card
- no parser code movement yet

## Non-Goals

- no public CLI behavior changes
- no parser rewrites
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the parser hotspot inventory is recorded and the
first implementation card is ready.

## Validation

- PASS: parser file line-count scan
  - `crates/effigy-cli/src/command_parsing.rs`: 1846 lines
  - `crates/effigy-cli/src/command_parsing_container.rs`: 777 lines
  - `crates/effigy-cli/src/lib.rs`: 864 lines
  - existing split parser modules are below target:
    - `command_parsing_demo.rs`: 580 lines
    - `command_parsing_docs.rs`: 458 lines
    - `command_parsing_distribution.rs`: 446 lines
- PASS: parse-test inventory scan
  - artifact: `artifact_option_tests.rs`, 4 focused tests
  - bootstrap: `bootstrap_option_tests.rs`, 15 focused tests
  - container/cache/data: `catalog_and_container_option_tests.rs`, focused
    coverage for status/down/stats/reset/cache/data/export/dump/import/pull/seed
- PASS: `git diff --check`

## Next Task

Extract artifact parser module.
