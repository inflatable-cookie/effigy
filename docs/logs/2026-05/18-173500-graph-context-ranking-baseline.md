# Graph Context Ranking Baseline

Date: 2026-05-18  
Roadmap: [`g07.026`](../../roadmaps/g07/026-context-ranking-baseline-and-gold-tasks.md)  
Batch card: [`971`](../../roadmaps/g07/batch-cards/971-baseline-context-ranking-quality.md)  
Strict lane: [`089`](../../specs/089-graph-navigation-ranking-quality-strict-lane.md)

## What Changed

- locked the first graph navigation quality baseline
- added a regression that records the current generic-query ranking gap
- defined the failure set for the role-aware ranker work in `972`

## Gold Tasks

### `trace deploy provider export`

Expected owner files:

- `src/runner/deploy_command/provider_package.rs`
- `src/runner/deploy_command/mod.rs`
- `src/runner/deploy_command/transaction.rs`

Current behavior:

- ranks `provider_package.rs` first
- includes deploy transaction/model/report files early
- includes deploy tests at rank 2

Assessment:

- useful today
- tests are a little high but do not hide the primary implementation owner

### `trace graph watch implementation`

Expected owner files:

- `crates/effigy-codegraph/src/watch.rs`
- `src/cli/graph_watch_dispatch.rs`
- `crates/effigy-cli/src/command_parsing_graph.rs`

Current behavior:

- over-ranks graph tests and broad graph files because `graph` and `watch` are
  common terms
- implementation files are discoverable through `rg`, but context ranking does
  not reliably choose them first

Assessment:

- this is the main failure for `972`
- implementation intent should down-rank tests/docs unless explicitly requested

### `understand release orchestration`

Expected owner files:

- `crates/effigy-release/src/lib.rs`
- `crates/effigy-release/src/model.rs`
- `src/runner/release_command/*`

Current behavior:

- broad `release` matching is noisy
- CLI output tests can dominate because they contain many release test symbols

Assessment:

- repeated same-file symbol hits inflate scores
- context should cap repeated token hits per file

### `find state capture profile resolution`

Expected owner files:

- `crates/effigy-state/src/config.rs`
- `src/runner/state_command.rs`

Current behavior:

- not yet locked as a test, but this task should be part of closeout proof

Assessment:

- useful for checking runner-versus-domain ownership ranking after `972`

### `docs for graph agent workflow`

Expected owner files:

- `docs/guides/076-code-graph-and-agent-workflows.md`
- `skills/effigy/SKILL.md`

Current behavior:

- docs should rank first when request intent explicitly asks for docs, guide,
  skill, contract, or examples

Assessment:

- role-aware ranking must be a boost/penalty model, not a global docs
  suppression model

## Regression Added

Added `graph_context_baseline_exposes_generic_query_ranking_gap`.

The fixture contains:

- implementation file: `src/graph/watch.rs`
- test file with many graph-watch symbols: `tests/graph_watch_tests.rs`
- docs file: `docs/graph-watch.md`

The test records current behavior:

- implementation file is present
- test file ranks first for `trace graph watch implementation`

This test should be changed in `972` so implementation ranks first.

## `rg` Positioning

`rg` remains better for exact text lookup.

Graph should win only when the agent needs a bounded owned-file starting set.
The closeout should keep this distinction explicit instead of overselling graph
search.

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved: graph ranking quality from anecdotal observation to a test-backed
  baseline
- remains open: `972`, `973`, `974`
