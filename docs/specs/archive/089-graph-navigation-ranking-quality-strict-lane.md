# 089 - Graph Navigation Ranking Quality Strict Lane

Roadmap: [`g07.025`](../roadmaps/g07/025-graph-context-ranking-quality-suite.md)
Related planning:
- [`g07.026`](../roadmaps/g07/026-context-ranking-baseline-and-gold-tasks.md)
- [`g07.027`](../roadmaps/g07/027-role-aware-context-ranker.md)
- [`g07.028`](../roadmaps/g07/028-search-and-snippet-usefulness.md)
- [`g07.029`](../roadmaps/g07/029-graph-navigation-quality-closeout.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Improve graph navigation quality so agents can use `effigy graph context` as a
high-signal first read before broad filesystem scans.

## Lane Posture

Posture: `strict-closed`

This lane is justified by measured behavior:

- exact `rg` remains much faster for simple token lookup
- `graph context "trace deploy provider export"` returns a useful owned-file
  set
- `graph context "trace graph watch implementation"` over-ranks tests and broad
  graph files before the actual watch implementation

## Hard Boundaries

- no embeddings or LLM-generated summaries
- no MCP or daemon changes
- no replacement claim for `rg`
- no hardcoded Effigy-only path ranking
- no breaking JSON contract changes
- no subjective closeout without stable gold-task tests

## Execution Order

1. `970` complete: ranking-quality lane opened
2. `971` complete: baseline context ranking quality
3. `972` complete: implement role-aware context ranking
4. `973` complete: improve search and context snippets
5. `974` complete: close navigation quality proof

## Ready Chain

- no ready card remains

## Auto-Continuation Envelope

Auto-start is enabled while:

- each card closes with tests or a documented planning artifact
- graph context remains deterministic and explainable
- changes are bounded to ranking, snippets, search projection, and docs

Stop and replan if:

- useful ranking requires semantic analysis beyond current graph facts
- fixes depend on project-specific path hacks
- JSON contract changes would break existing consumers

## Acceptance

This lane is complete when:

- gold tasks prove context ranking quality improved
- implementation/test/docs intent is handled predictably
- snippets point near useful evidence
- residual `rg` superiority cases are documented

## Next Task

No active task remains in lane `089`.
