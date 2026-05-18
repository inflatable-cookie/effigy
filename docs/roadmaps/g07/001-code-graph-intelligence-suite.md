# g07.001 - Code Graph Intelligence Suite

Status: Complete
Depends on: none

## Goal

Add a native code graph surface that gives agents a fast, deterministic map of
the repo before they scan files directly.

The feature should answer practical navigation questions:

- which files define this behavior?
- what symbols are available?
- what imports or calls point here?
- what Effigy tasks, manifests, docs, and bundles relate to this work?
- what bounded context pack should an agent read first?

## Product Shape

The first public surface should be CLI-first:

- `effigy graph index`
- `effigy graph status --json`
- `effigy graph search <query> --json`
- `effigy graph files --json`
- `effigy graph node <id> --json`
- `effigy graph callers <id> --json`
- `effigy graph callees <id> --json`
- `effigy graph impact <path|symbol> --json`
- `effigy graph context "<task>" --json`

The graph DB is an implementation detail. JSON command output is the reusable
contract for agents.

## Ordered Follow-Up Lanes

1. [`002-graph-storage-and-json-contracts.md`](./002-graph-storage-and-json-contracts.md)
2. [`003-graph-index-command-and-freshness-model.md`](./003-graph-index-command-and-freshness-model.md)
3. [`004-first-party-language-extractor-framework.md`](./004-first-party-language-extractor-framework.md)
4. [`005-rust-extractor.md`](./005-rust-extractor.md)
5. [`006-effigy-manifest-toml-and-task-graph-indexer.md`](./006-effigy-manifest-toml-and-task-graph-indexer.md)
6. [`007-markdown-docs-and-anchor-indexer.md`](./007-markdown-docs-and-anchor-indexer.md)
7. [`008-php-extractor.md`](./008-php-extractor.md)
8. [`009-javascript-typescript-extractor.md`](./009-javascript-typescript-extractor.md)
9. [`010-query-commands.md`](./010-query-commands.md)
10. [`011-agent-context-packs.md`](./011-agent-context-packs.md)
11. [`012-performance-cache-and-regression-proof.md`](./012-performance-cache-and-regression-proof.md)

## Execution Guardrails

- no MCP server in this suite
- no JavaScript runtime dependency
- no external language plugin/package system in v1
- no background daemon
- no compiler-grade semantic claims
- no LLM summary as canonical graph data
- all graph facts must carry source path, source range where available,
  extractor owner, and confidence/provenance
- generated/cache/vendor directories must be excluded by default unless a
  specific extractor or command opts in
- every JSON response must include schema and version fields

## Acceptance Criteria

- agents can query a fresh index without direct broad file scanning
- graph artifacts live under `.effigy/graph/`
- stale index state is visible and actionable
- Effigy can index itself enough to answer high-signal navigation questions
- first-party language extractors are modular and tested independently
- closeout compares graph-assisted lookup against direct `rg` exploration

## Batch Cards

- [`900-open-code-graph-intelligence-lane.md`](./batch-cards/900-open-code-graph-intelligence-lane.md)
- [`901-baseline-storage-query-design-and-dependency-spike.md`](./batch-cards/901-baseline-storage-query-design-and-dependency-spike.md)
- [`902-implement-graph-storage-and-json-contracts.md`](./batch-cards/902-implement-graph-storage-and-json-contracts.md)
- [`903-implement-graph-index-status-and-freshness.md`](./batch-cards/903-implement-graph-index-status-and-freshness.md)
- [`904-implement-first-party-extractor-framework.md`](./batch-cards/904-implement-first-party-extractor-framework.md)
- [`905-implement-rust-extractor.md`](./batch-cards/905-implement-rust-extractor.md)
- [`906-implement-effigy-manifest-toml-task-indexer.md`](./batch-cards/906-implement-effigy-manifest-toml-task-indexer.md)
- [`907-implement-markdown-docs-anchor-indexer.md`](./batch-cards/907-implement-markdown-docs-anchor-indexer.md)
- [`908-implement-php-extractor.md`](./batch-cards/908-implement-php-extractor.md)
- [`909-implement-javascript-typescript-extractor.md`](./batch-cards/909-implement-javascript-typescript-extractor.md)
- [`910-implement-graph-query-commands.md`](./batch-cards/910-implement-graph-query-commands.md)
- [`911-implement-agent-context-packs.md`](./batch-cards/911-implement-agent-context-packs.md)
- [`912-close-code-graph-intelligence-proof.md`](./batch-cards/912-close-code-graph-intelligence-proof.md)

## Next Task

No active batch card. Open the next graph tranche from the residual limits.
