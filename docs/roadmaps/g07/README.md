# g07 Roadmaps

Status: Active
Theme: Native code graph intelligence for agents

## Purpose

`g07` plans Effigy's native code graph surface.

The goal is a deterministic, local, queryable interpretation of a repo that
agents can use before falling back to broad file scans. The graph should make
Effigy better at answering "where is this behavior owned?", "what calls this?",
"what files matter for this task?", and "what context should I read first?"

The CLI remains the protocol. This generation does not introduce an MCP server,
a background daemon, external language plugins, or a JavaScript runtime
dependency.

## Roadmap Sequence

- [`001-code-graph-intelligence-suite.md`](./001-code-graph-intelligence-suite.md)
- [`002-graph-storage-and-json-contracts.md`](./002-graph-storage-and-json-contracts.md)
- [`003-graph-index-command-and-freshness-model.md`](./003-graph-index-command-and-freshness-model.md)
- [`004-first-party-language-extractor-framework.md`](./004-first-party-language-extractor-framework.md)
- [`005-rust-extractor.md`](./005-rust-extractor.md)
- [`006-effigy-manifest-toml-and-task-graph-indexer.md`](./006-effigy-manifest-toml-and-task-graph-indexer.md)
- [`007-markdown-docs-and-anchor-indexer.md`](./007-markdown-docs-and-anchor-indexer.md)
- [`008-php-extractor.md`](./008-php-extractor.md)
- [`009-javascript-typescript-extractor.md`](./009-javascript-typescript-extractor.md)
- [`010-query-commands.md`](./010-query-commands.md)
- [`011-agent-context-packs.md`](./011-agent-context-packs.md)
- [`012-performance-cache-and-regression-proof.md`](./012-performance-cache-and-regression-proof.md)
- [`013-graph-follow-up-performance-and-fixture-reliability.md`](./013-graph-follow-up-performance-and-fixture-reliability.md)
- [`014-incremental-indexing-and-cache-reuse.md`](./014-incremental-indexing-and-cache-reuse.md)
- [`015-query-speed-and-projection-reduction.md`](./015-query-speed-and-projection-reduction.md)
- [`016-failed-graph-fixture-path-reliability.md`](./016-failed-graph-fixture-path-reliability.md)
- [`017-graph-scan-cost-reduction-suite.md`](./017-graph-scan-cost-reduction-suite.md)
- [`018-file-walk-and-scan-metadata-baseline.md`](./018-file-walk-and-scan-metadata-baseline.md)
- [`019-safe-scan-metadata-reuse.md`](./019-safe-scan-metadata-reuse.md)
- [`020-scan-cost-closeout-proof.md`](./020-scan-cost-closeout-proof.md)
- [`021-graph-watch-mode-suite.md`](./021-graph-watch-mode-suite.md)
- [`022-watch-backend-and-debounce-rules.md`](./022-watch-backend-and-debounce-rules.md)
- [`023-dirty-reconcile-and-overflow-fallback.md`](./023-dirty-reconcile-and-overflow-fallback.md)
- [`024-graph-watch-closeout-proof.md`](./024-graph-watch-closeout-proof.md)

## Design Posture

- keep all v1 language support first-party and compiled into Effigy
- use internal extractor traits so language owners stay modular
- do not claim compiler-grade semantic analysis
- mark heuristic edges as heuristic
- store provenance and ranges for every emitted graph fact
- treat JSON CLI output as the public contract, not the DB layout
- keep graph artifacts local under `.effigy/graph/`

## Non-Goals

- no MCP server for graph v1
- no graph-specific daemon
- no JavaScript runtime dependency
- no external language package/plugin system
- no LLM-generated summaries as canonical graph data
- no editor-specific integration as core scope
- no "support every language" launch target

## Current State

`g06` is closed through `g06.008`.

`g07.001` is complete.

`g07.013` is complete.

`g07.017` is complete.

`g07.021` is complete.

`g07.022` is complete.

`g07.023` is complete.

`g07.024` is complete.

`901` through `905` are complete.

`906` through `910` are complete.

`911` is complete.

`912` is complete.

`931` is complete.

`932` is complete.

`933` is complete.

`935` is complete.

`950` through `954` are complete.

`960` is complete.

`961` is complete.

`962` is complete.

`963` is complete.

`964` is complete.

No active `g07` batch card remains.

## Next Task

Leave `g07` parked unless a later graph tranche justifies more work.
