# Graph Navigation Quality Closeout

Date: 2026-05-18  
Roadmap: [`g07.029`](../../../roadmaps/g07/029-graph-navigation-quality-closeout.md)  
Batch card: [`974`](../../../roadmaps/g07/batch-cards/974-close-graph-navigation-quality-proof.md)  
Strict lane: [`089`](../../../specs/089-graph-navigation-ranking-quality-strict-lane.md)

## What Changed

- closed the graph navigation ranking-quality lane
- recorded before/after evidence for role-aware context ranking
- kept `rg` positioning explicit for exact text lookup

## Before

The baseline showed:

- `trace deploy provider export` was already useful, but tests could rank high
- `trace graph watch implementation` over-ranked tests and broad graph files
- repeated same-file symbol matches inflated noisy files
- `graph search` returned record IDs without snippets
- file-level context snippets started at file top

## After

Current live Effigy repo checks:

### `graph context "trace graph watch implementation" --language rust`

- rank 1: `crates/effigy-codegraph/src/watch.rs`
- rank 2: `src/cli/graph_watch_dispatch.rs`
- rank 3: `crates/effigy-cli/src/command_parsing_graph.rs`
- timing: `1.79s`

### `graph context "trace deploy provider export" --language rust`

- rank 1: `src/runner/deploy_command/provider_context.rs`
- rank 2: `src/runner/deploy_command/provider_package.rs`
- rank 3: `crates/effigy-cli/src/command_parsing_deploy.rs`
- rank 4: `src/runner/deploy_command/derive.rs`
- rank 5: `src/runner/deploy_command/mod.rs`
- rank 6: `src/runner/deploy_command/model.rs`
- deploy tests no longer rank in the top six
- timing: `1.79s`

### `graph search watch_repo --limit 3`

- returns `watch_repo`
- path: `crates/effigy-codegraph/src/watch.rs`
- includes a function-level snippet
- timing: `0.39s`

### `rg -n "watch_repo" crates src tests`

- returns `9` exact text hits
- timing: `0.03s`

## Conclusion

Graph context is now useful as a first owned-file narrowing pass.

Graph search is more actionable because it includes snippets, but exact text
lookup still belongs to `rg`.

Recommended workflow remains:

1. use `graph context` to choose the first files to read
2. use `graph node`, `callers`, `callees`, or `impact` after a relevant record
   is known
3. use `rg` for exact text lookup and verification

## Residual Limits

- snippets are lexical/symbol-driven, not semantic
- generic short queries can still include adjacent command families
- `graph search` remains secondary to `rg` for exact token lookup
- no further graph-navigation tranche is open

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved: graph context quality is now test-backed and useful enough for
  agent-first navigation
- remains open: none in lane `089`
