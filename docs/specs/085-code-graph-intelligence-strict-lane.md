# 085 - Code Graph Intelligence Strict Lane

Roadmap: [`g07.001`](../roadmaps/g07/001-code-graph-intelligence-suite.md)
Related planning:
- [`g07.002`](../roadmaps/g07/002-graph-storage-and-json-contracts.md)
- [`g07.003`](../roadmaps/g07/003-graph-index-command-and-freshness-model.md)
- [`g07.004`](../roadmaps/g07/004-first-party-language-extractor-framework.md)
- [`g07.005`](../roadmaps/g07/005-rust-extractor.md)
- [`g07.006`](../roadmaps/g07/006-effigy-manifest-toml-and-task-graph-indexer.md)
- [`g07.007`](../roadmaps/g07/007-markdown-docs-and-anchor-indexer.md)
- [`g07.008`](../roadmaps/g07/008-php-extractor.md)
- [`g07.009`](../roadmaps/g07/009-javascript-typescript-extractor.md)
- [`g07.010`](../roadmaps/g07/010-query-commands.md)
- [`g07.011`](../roadmaps/g07/011-agent-context-packs.md)
- [`g07.012`](../roadmaps/g07/012-performance-cache-and-regression-proof.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Execute the native code graph intelligence tranche without turning Effigy into
an agent server, a language-plugin host, or a compiler replacement.

This lane exists to give agents a deterministic local graph they can query
through Effigy CLI JSON before they fall back to broad source scanning.

## Lane Posture

Posture: `strict-complete`

This lane is executable because the product boundaries are explicit:

- CLI JSON is the protocol
- graph artifacts are local
- extractors are first-party in v1
- language facts are syntax-derived with confidence/provenance
- no MCP, daemon, plugin package runtime, or JavaScript runtime dependency is
  in scope

## Hard Boundaries

- no MCP server
- no graph daemon
- no external language package/plugin runtime in v1
- no JavaScript runtime dependency
- no LLM-generated summary as canonical graph data
- no compiler-grade semantic claims
- no hidden indexing of `target`, `node_modules`, `vendor`, `.git`, or runtime
  cache directories by default
- no graph DB writes from language extractors directly
- no public contract without schema/version fields
- no release protocol weakening

## Execution Order

1. `900` complete: lane opened and ready chain wired
2. `901` complete: baseline storage/query design and dependency spike
3. `902` complete: graph storage and JSON contracts
4. `903` complete: graph index/status command and freshness model
5. `904` complete: first-party extractor framework
6. `905` complete: Rust extractor
7. `906` complete: Effigy manifest/TOML/task graph indexer hardening
8. `907` complete: Markdown docs and anchor indexer depth pass
9. `908` complete: PHP extractor proof pass
10. `909` complete: JavaScript/TypeScript extractor proof pass
11. `910` complete: query commands
12. `911` complete: agent context packs
13. `912` planned: performance/cache/regression closeout

## Ready Chain

- `900` through `912` are complete
- no active ready card remains in this lane

## Auto-Continuation Envelope

Auto-start is enabled for this lane while:

- the previous card closes green
- the next card has a bounded implementation surface
- JSON contracts remain explicit
- extractor claims stay confidence-scoped
- no server/plugin/runtime dependency is introduced

Stop and replan if implementation discovers:

- a language extractor needs typechecker/compiler integration to be useful
- the DB schema wants to become the public contract
- query output cannot stay bounded
- generated or vendor paths are needed by default
- agent-context ranking wants LLM scoring in v1
- tree-sitter crate cost makes the first-party extractor plan untenable

## Acceptance

This lane is complete when:

- graph artifacts are local and versioned
- index/status/query/context commands have strict JSON contracts
- first-party extractors produce provenance-backed graph facts
- Effigy can index itself and provide useful agent context packs
- performance and limitations are recorded in closeout evidence

## Next Task

No active lane work. Open the next graph tranche from the closeout limits.
