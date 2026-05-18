# g07.026 - Context Ranking Baseline And Gold Tasks

Status: Complete
Depends on: `g07.025`

## Goal

Turn graph usefulness into a measurable contract before changing ranking code.

## Scope

- define 5 to 8 gold navigation tasks against the Effigy repo
- record expected top files for each task
- measure `graph context`, `graph search`, and direct `rg` for each task
- identify which failures are ranking bugs versus expected `rg` superiority
- add focused tests that can fail before the implementation work lands

## Candidate Gold Tasks

- `trace deploy provider export`
  - should surface `src/runner/deploy_command/provider_package.rs`
  - should surface `src/runner/deploy_command/mod.rs`
  - tests may appear, but should not outrank primary implementation
- `trace graph watch implementation`
  - should surface `crates/effigy-codegraph/src/watch.rs`
  - should surface `src/cli/graph_watch_dispatch.rs`
  - graph tests should not be rank 1 for implementation wording
- `understand release orchestration`
  - should surface `crates/effigy-release/src/lib.rs` or release owner modules
  - should surface `src/runner/release_command/*`
  - CLI output tests should not dominate rank 1 by symbol count
- `find state capture profile resolution`
  - should surface `crates/effigy-state/src/config.rs`
  - should surface `src/runner/state_command.rs` only as runner adapter context
- `docs for graph agent workflow`
  - should surface `docs/guides/076-code-graph-and-agent-workflows.md`
  - docs should outrank implementation because request intent is docs

## Acceptance Criteria

- baseline log records current ranks and timings
- tests encode expected rank direction without overfitting exact numeric scores
- the next implementation card has a concrete failure set to close

## Next Task

After `971`, execute `972`.
