# Documentation Graph Profile Planning

Status: complete
Created: 2026-08-29
Roadmap: g08.035
Batch: planning compile

## Summary

- Selected the agent-native maintainer theme as a narrow `g08` extension.
- Recorded the selection as vision decision `D-2026-04`.
- Defined a generic repository-owned docs graph profile under `docs_policy`.
- Kept Northstar as a committed starter profile rather than runtime authority.
- Compiled strict spec `108` and ready card `1088`.

## Evidence

- The current Markdown graph already indexes documents, headings, links, code
  fences, local path references, and FTS source.
- Heading symbols currently carry whole-file spans, so exact section extraction
  is the first structural gap.
- Existing graph search can find relevant contracts, but completed roadmaps,
  handoffs, and unrelated docs remain noisy because currentness and authority
  are not represented.
- `ManifestDocsPolicyConfig` already provides the repository-owned manifest
  seam used by documentation checks.
- The Northstar starter already commits docs-policy configuration into consumer
  repositories, making it the natural profile delivery boundary.

## Decisions

- Reuse `.effigy/graph/graph.db`, graph freshness, FTS, symbols, and edges.
- Baseline mode works without a profile.
- Repositories define roots, arbitrary fields, currentness values, kinds,
  authority weights, and relation selectors.
- `effigy docs context` returns exact bounded evidence, not generated answers.
- No embeddings, daemon, MCP requirement, or skill-directory runtime lookup in
  the first lane.

## Traceability

- [`D-2026-04`](../../vision/decisions/D-2026-04-repository-defined-documentation-graph.md)
- [`architecture 024`](../../architecture/024-repository-defined-documentation-graph.md)
- [`contract 041`](../../contracts/041-documentation-graph-profile-contract.md)
- [`strict spec 108`](../../specs/archive/108-documentation-graph-profiles-strict-lane.md)

## Vision Target Delta

- Tags: `OPERATE`, `MAINT`, `ROUTE`, `CONTRACT`
- Baseline: code-oriented graph navigation with structurally indexed Markdown.
- Current state: architecture, contract, strict lane, and ready batch define a
  repository-neutral semantic documentation graph.
- Open: cards `1088` through `1090` implementation and proof.

## Next Task

Execute ready card
[`1088`](../../roadmaps/g08/batch-cards/1088-build-documentation-profile-and-structural-index.md).
