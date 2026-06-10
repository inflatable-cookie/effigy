# Graph Search And Context Snippet Usefulness

Date: 2026-05-18  
Roadmap: [`g07.028`](../../../roadmaps/g07/028-search-and-snippet-usefulness.md)  
Batch card: [`973`](../../../roadmaps/g07/batch-cards/973-improve-search-and-context-snippets.md)  
Strict lane: [`089`](../../../specs/089-graph-navigation-ranking-quality-strict-lane.md)

## What Changed

- `graph search` now resolves file and symbol matches into actionable snippets
- file-level `graph context` items now prefer snippets near matched symbol
  evidence when available
- file-level context items now carry the evidence range when a matched symbol
  drove the selection
- byte budgets and truncation accounting remain unchanged

## Evidence

Focused regression coverage:

- `graph_context_file_snippets_start_near_matched_symbol_evidence`
- `graph_search_returns_actionable_symbol_snippets`

Live Effigy repo check after rebuild and reindex:

- `graph context "trace graph watch implementation" --language rust`
  - rank 1: `crates/effigy-codegraph/src/watch.rs`
  - snippet is near graph-watch symbol evidence, not the top of the file
  - rank 2: `src/cli/graph_watch_dispatch.rs`
  - snippet is near `emit_watch_event`
- `graph search watch_repo --limit 3`
  - returns `watch_repo`
  - includes `crates/effigy-codegraph/src/watch.rs`
  - includes a function-level snippet

## Residual Limits

- snippet selection is still lexical and symbol-driven, not semantic
- exact text lookup remains an `rg` job
- final before/after proof remains `974`

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved: graph outputs are easier for agents to act on after ranking selects a
  useful record
- remains open: `974`
