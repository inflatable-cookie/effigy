# 051 - CLI Parser Modularisation For Runtime Surfaces Strict Lane

Roadmap: [`g04.009`](../roadmaps/g04/009-cli-parser-modularisation-for-runtime-surfaces.md)

Status: Active
Owner: Platform
Created: 2026-05-07

## Purpose

Reduce parser centralisation for high-churn runtime/container features while
keeping the public `effigy-cli` command model stable.

## Hard Boundaries

- preserve public CLI behavior unless a card explicitly selects a cleanup break
- keep `Command` enum shape stable unless a break is documented
- avoid broad parser rewrites
- add focused parse coverage for moved surfaces
- no release work
- no `.github/workflows/` edits

## Current Ready Card

[`567-extract-release-parser-module.md`](./batch-cards/567-extract-release-parser-module.md)

## Execution Chain

- `561` complete: close manager-backed runtime read/write/shell
- `562` complete: scaffold CLI parser modularisation lane
- `563` complete: extract artifact parser module
- `564` complete: extract bootstrap parser module
- `565` complete: extract container data parser module
- `566` complete: extract runtime surface parser module
- `567` ready: extract release parser module

## Parser Hotspot Inventory

- `crates/effigy-cli/src/command_parsing.rs`: 1846 lines
- `crates/effigy-cli/src/command_parsing_container.rs`: 777 lines
- `crates/effigy-cli/src/lib.rs`: 864 lines
- existing split parser modules:
  - `command_parsing_demo.rs`: 580 lines
  - `command_parsing_docs.rs`: 458 lines
  - `command_parsing_distribution.rs`: 446 lines
  - `command_parsing_deploy.rs`: 105 lines

## Parse Coverage Inventory

- artifact: `src/tests/lib_tests_parse_tests/artifact_option_tests.rs`
  covers inspect, stage, capture, and help
- bootstrap: `src/tests/lib_tests_parse_tests/bootstrap_option_tests.rs`
  covers help, plan/default start/no-start, db seed variants, no-prompt,
  reuse-path, fresh, backend, teardown, deps sync, children sync/status
- container/cache/data:
  `src/tests/lib_tests_parse_tests/catalog_and_container_option_tests.rs`
  covers lifecycle/read/reset/cache/data/export/dump/import/pull/seed paths

First implementation slice:

Extract artifact parsing first. It is self-contained, already has focused parse
coverage, and removes a runtime-adjacent feature from the root parser without
touching container data parsing yet.

Second implementation slice:

Extract bootstrap parsing next. It has focused parse coverage and removes a
large contiguous runtime-adjacent parser block from the root parser. Keep the
DB seed value parser shared with container data seed parsing.

Third implementation slice:

Extract container data parsing. It is the largest high-churn block left in the
container parser and should put `command_parsing_container.rs` under the
roadmap threshold while preserving the shared DB seed parser behavior.

Fourth implementation slice:

Extract the remaining runtime-adjacent built-ins from the root parser:
`exec`, `system`, `workspace`, `gateway`, and `service`. This continues the
same ownership pattern without changing public command behavior.

Fifth implementation slice:

Extract release parsing. It is a large, well-covered block still owned by the
root parser. This should bring `command_parsing.rs` close to the roadmap target;
split changelog next only if the root parser remains too large.

## Initial Targets

- `crates/effigy-cli/src/command_parsing.rs`
- `crates/effigy-cli/src/command_parsing_container.rs`
- `crates/effigy-cli/src/lib.rs`
- `src/tests/lib_tests_parse_tests/*`

## Exit Condition

This lane closes when high-churn runtime/container parser surfaces have focused
module ownership and parse coverage, parser files are below the roadmap target
or have a documented reason to stay larger, and the next roadmap is selected.

## Next Task

Card
[`567-extract-release-parser-module.md`](./batch-cards/567-extract-release-parser-module.md).
