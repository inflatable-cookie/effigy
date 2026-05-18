# g07.025 - Graph Context Ranking Quality Suite

Status: Complete
Depends on: `g07.001` through `g07.024`

## Goal

Make `effigy graph context` reliably useful as the first agent navigation step
before broad filesystem scanning.

The graph does not need to beat `rg` for exact text lookup. It does need to
beat broad, blind scanning when an agent asks an engineering-task question such
as:

- "trace deploy provider export"
- "trace graph watch implementation"
- "understand release orchestration"
- "find where manifest state capture is resolved"

## Why This Exists

The current graph implementation is usable but uneven.

Observed strengths:

- no-op indexing is cheap enough for normal use
- `graph context "trace deploy provider export"` ranks the correct deploy
  implementation files first
- the graph is better than raw grep when the user wants a bounded starting set

Observed weaknesses:

- exact text lookup is still much faster with `rg`
- generic context requests can over-rank tests, docs, or high-symbol-count files
- repeated symbol hits inflate file scores
- context snippets often start at file top instead of the matched symbol or
  reason-bearing region
- `graph search` is weaker than `rg` for broad tokens and does not explain
  enough to be a good agent entry point

## Scope

- define a small gold-task evaluation set for graph context quality
- add role-aware ranking for implementation, test, docs, roadmap, generated, and
  fixture files
- normalize tokens and reduce generic-token noise
- cap repeated reason inflation from many symbols in one file
- prefer exact phrase and multi-token co-occurrence over isolated keyword hits
- improve snippets so context points near matched symbols or path evidence
- clarify search-versus-context guidance through docs and tests

## Non-Goals

- do not try to replace `rg`
- do not introduce embeddings, LLM summaries, or remote inference
- do not claim compiler-grade semantic analysis
- do not add a daemon or MCP surface
- do not make ranking depend on Effigy-specific hardcoded path lists beyond
  generic role classification

## Hard Boundaries

- JSON schemas may add optional fields only if needed; do not break existing
  graph payloads
- all scoring reasons must stay explainable in output
- ranking changes must be covered by stable fixture tests, not subjective manual
  inspection only
- do not hide tests/docs globally; rank them lower unless the request indicates
  tests, docs, examples, fixtures, or contracts
- keep direct `rg` as the recommended exact-token tool

## Ordered Follow-Up Lanes

1. [`026-context-ranking-baseline-and-gold-tasks.md`](./026-context-ranking-baseline-and-gold-tasks.md)
2. [`027-role-aware-context-ranker.md`](./027-role-aware-context-ranker.md)
3. [`028-search-and-snippet-usefulness.md`](./028-search-and-snippet-usefulness.md)
4. [`029-graph-navigation-quality-closeout.md`](./029-graph-navigation-quality-closeout.md)

## Acceptance Criteria

- gold tasks produce stable top-file sets that match expected ownership
- implementation files outrank tests/docs for implementation questions
- tests/docs outrank implementation when the request explicitly asks for tests
  or docs
- repeated same-file symbol hits no longer dominate ranking
- context snippets are near the best evidence when possible
- closeout compares graph context, graph search, and `rg` honestly

## Batch Cards

- [`970-open-graph-ranking-quality-lane.md`](./batch-cards/970-open-graph-ranking-quality-lane.md)
- [`971-baseline-context-ranking-quality.md`](./batch-cards/971-baseline-context-ranking-quality.md)
- [`972-implement-role-aware-context-ranking.md`](./batch-cards/972-implement-role-aware-context-ranking.md)
- [`973-improve-search-and-context-snippets.md`](./batch-cards/973-improve-search-and-context-snippets.md)
- [`974-close-graph-navigation-quality-proof.md`](./batch-cards/974-close-graph-navigation-quality-proof.md)

## Next Task

No active ranking-quality task remains.
