# Role-Aware Graph Context Ranking

Date: 2026-05-18  
Roadmap: [`g07.027`](../../../roadmaps/g07/027-role-aware-context-ranker.md)  
Batch card: [`972`](../../../roadmaps/g07/batch-cards/972-implement-role-aware-context-ranking.md)  
Strict lane: [`089`](../../../specs/089-graph-navigation-ranking-quality-strict-lane.md)

## What Changed

- added generic file-role classification for implementation, tests, docs,
  planning, fixtures, and generated paths
- added request-intent classification for implementation, test, docs, and
  general queries
- removed high-noise context verbs from direct match scoring after they have
  contributed to intent detection
- capped repeated symbol-hit and doc-link score inflation per file
- kept scoring reasons visible in context payloads

## Evidence

Focused regression coverage:

- `graph_context_ranks_implementation_before_tests_for_implementation_requests`
- `graph_context_ranks_tests_and_docs_when_request_intent_asks_for_them`

Live Effigy repo check after rebuilding and reindexing:

- `graph context "trace graph watch implementation" --language rust`
  - rank 1: `crates/effigy-codegraph/src/watch.rs`
  - rank 2: `src/cli/graph_watch_dispatch.rs`
  - rank 3: `crates/effigy-cli/src/command_parsing_graph.rs`
- `graph context "trace deploy provider export" --language rust`
  - top six are deploy implementation files
  - deploy tests no longer rank in the top six

## Residual Limits

- `graph search` still needs better actionability; that remains `973`
- context snippets still start at file top for file-level results; that remains
  `973`
- exact text lookup remains an `rg` job

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved: generic graph context ranking now prefers likely owner files for
  implementation requests
- remains open: `973`, `974`
