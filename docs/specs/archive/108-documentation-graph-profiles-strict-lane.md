# 108 - Documentation Graph Profiles Strict Lane

Status: Archived
Archived: 2026-08-31
Owner: documentation graph and agent retrieval surfaces
Roadmap: [`g08.035`](../../roadmaps/g08/035-repository-defined-documentation-graph.md)
Architecture: [`024`](../../architecture/024-repository-defined-documentation-graph.md)
Contract: [`041`](../../contracts/041-documentation-graph-profile-contract.md)
Current ready card: none; the lane is complete

## Problem

The native graph can search Markdown, but it cannot represent a repository's
documentation authority, currentness, or project-specific relation types.
Northstar has a strong documentation shape, yet baking that vocabulary into
Effigy would make the feature less useful to other repositories and couple
runtime behavior to an agent framework.

## Decision

- Build a generic structural documentation graph on the existing code graph.
- Add an optional repository-owned profile under `[docs_policy.graph]`.
- Keep baseline retrieval useful without a profile.
- Treat Northstar as a committed starter profile, not a hard-coded ontology.
- Return bounded exact evidence with provenance and versioned JSON.
- Keep embeddings, generated summaries, MCP, and a daemon outside this lane.

## Execution Sequence

1. [`1088`](../../roadmaps/g08/batch-cards/1088-build-documentation-profile-and-structural-index.md)
   adds typed profile grammar, validation, profile freshness, and exact
   Markdown structure/facts.
2. [`1089`](../../roadmaps/g08/batch-cards/1089-add-bounded-documentation-context-query.md)
   adds deterministic retrieval, relation traversal, CLI/help, budgets, and
   JSON.
3. [`1090`](../../roadmaps/g08/batch-cards/1090-prove-generic-and-northstar-profiles.md)
   proves generic and Northstar configurations, publishes adoption guidance,
   runs proportional validation, and closes the lane.

Cards `1088`, `1089`, `1090`, `1091`, and the temporary external skill-task lane
card `1092` are complete. Card `1089` closed on 2026-08-31 with evidence
[`31-181957-documentation-context-1089.md`](../../logs/archive/2026-08/31-181957-documentation-context-1089.md).
Card `1090` closed on 2026-08-31 with evidence
[`31-213000-northstar-profile-proof-1090.md`](../../logs/archive/2026-08/31-213000-northstar-profile-proof-1090.md),
which closed this lane.

## Owner And Seam

- profile parsing and validation stay in `effigy-manifest`
- structural extraction, profile compilation, freshness, graph facts, query
  logic, and reports stay in `effigy-codegraph`
- parser/help and built-in routing remain thin shells
- starter and guide changes describe adoption; they do not become runtime
  configuration sources

Do not create a second documentation database or a second manifest parser.

## Acceptance

- [x] baseline mode works in a repository with no profile
- [x] custom repositories can name their own kinds, fields, relations, paths,
      statuses, and authority weights
- [x] exact section spans and profile facts are deterministic and provenance-rich
- [x] query relevance leads ranking; currentness and authority improve relevant
      ties without injecting unrelated documents
- [x] traversal and output remain inside explicit count, byte, and hop budgets
- [x] text and `effigy.docs.context.v1` JSON agree
- [x] Northstar is expressed by committed starter configuration only
- [x] generic and Northstar fixtures plus an Effigy benchmark prove retrieval
- [x] focused checks, docs QA, formatting, Clippy, and full Effigy QA pass at
      lane closeout

## Stop Conditions

Stop and return to planning if:

- a second graph store or daemon becomes necessary
- profile semantics require Northstar-specific fallback code
- correct extraction requires model inference
- authority scoring can outrank unrelated lexical evidence
- the public query cannot remain bounded and deterministic
- manifest composition leaves profile ownership ambiguous
- implementation needs a workflow edit or release mutation

## Next Task

None. This lane is archived. Durable rules live in contract
[`041`](../../contracts/041-documentation-graph-profile-contract.md) and
architecture [`024`](../../architecture/024-repository-defined-documentation-graph.md);
adoption guidance lives in
[`079`](../../guides/079-documentation-graph-profiles-and-context.md).
