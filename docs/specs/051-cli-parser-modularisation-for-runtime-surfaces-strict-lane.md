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

[`562-scaffold-cli-parser-modularisation-lane.md`](./batch-cards/562-scaffold-cli-parser-modularisation-lane.md)

## Execution Chain

- `561` complete: close manager-backed runtime read/write/shell
- `562` ready: scaffold CLI parser modularisation lane

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
[`562-scaffold-cli-parser-modularisation-lane.md`](./batch-cards/562-scaffold-cli-parser-modularisation-lane.md).
