# 108 - Documentation Graph Profiles Strict Lane

Status: Paused
Owner: documentation graph and agent retrieval surfaces
Roadmap: [`g08.035`](../roadmaps/g08/035-repository-defined-documentation-graph.md)
Architecture: [`024`](../architecture/024-repository-defined-documentation-graph.md)
Contract: [`041`](../contracts/041-documentation-graph-profile-contract.md)
Paused card: [`1089`](../roadmaps/g08/batch-cards/1089-add-bounded-documentation-context-query.md)

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

1. [`1088`](../roadmaps/g08/batch-cards/1088-build-documentation-profile-and-structural-index.md)
   adds typed profile grammar, validation, profile freshness, and exact
   Markdown structure/facts.
2. [`1089`](../roadmaps/g08/batch-cards/1089-add-bounded-documentation-context-query.md)
   adds deterministic retrieval, relation traversal, CLI/help, budgets, and
   JSON.
3. [`1090`](../roadmaps/g08/batch-cards/1090-prove-generic-and-northstar-profiles.md)
   proves generic and Northstar configurations, publishes adoption guidance,
   runs proportional validation, and closes the lane.

Card `1088` and the overlapping maintenance card `1091` are complete. The
operator paused card `1089` on 2026-08-31 while external skill-task execution
runs under strict spec `110`; `1090` remains pending behind card `1089`.

## Owner And Seam

- profile parsing and validation stay in `effigy-manifest`
- structural extraction, profile compilation, freshness, graph facts, query
  logic, and reports stay in `effigy-codegraph`
- parser/help and built-in routing remain thin shells
- starter and guide changes describe adoption; they do not become runtime
  configuration sources

Do not create a second documentation database or a second manifest parser.

## Acceptance

- [ ] baseline mode works in a repository with no profile
- [ ] custom repositories can name their own kinds, fields, relations, paths,
      statuses, and authority weights
- [ ] exact section spans and profile facts are deterministic and provenance-rich
- [ ] query relevance leads ranking; currentness and authority improve relevant
      ties without injecting unrelated documents
- [ ] traversal and output remain inside explicit count, byte, and hop budgets
- [ ] text and `effigy.docs.context.v1` JSON agree
- [ ] Northstar is expressed by committed starter configuration only
- [ ] generic and Northstar fixtures plus an Effigy benchmark prove retrieval
- [ ] focused checks, docs QA, formatting, Clippy, and full Effigy QA pass at
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

Execute external skill-runner card
[`1092`](../roadmaps/g08/batch-cards/1092-add-external-skill-task-runner.md).
Resume [`1089`](../roadmaps/g08/batch-cards/1089-add-bounded-documentation-context-query.md)
after that lane closes.
