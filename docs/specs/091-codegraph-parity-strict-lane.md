# 091 - CodeGraph Parity Strict Lane

Roadmap: [`g07.035`](../roadmaps/g07/035-codegraph-parity-suite.md)
Related planning:
- [`g07.036`](../roadmaps/g07/036-parity-benchmark-harness-and-claim-discipline.md)
- [`g07.037`](../roadmaps/g07/037-fts-backed-source-evidence-and-ranking.md)
- [`g07.038`](../roadmaps/g07/038-traversal-aware-explore-assembly.md)
- [`g07.039`](../roadmaps/g07/039-richer-language-extractor-coverage.md)
- [`g07.040`](../roadmaps/g07/040-framework-route-and-entrypoint-edges.md)
- [`g07.041`](../roadmaps/g07/041-source-section-packets-and-no-reread-workflow.md)
- [`g07.042`](../roadmaps/g07/042-affected-test-and-impact-workflow.md)
- [`g07.043`](../roadmaps/g07/043-large-repo-scale-and-storage-hardening.md)
- [`g07.044`](../roadmaps/g07/044-agent-adoption-and-cli-workflow-polish.md)
- [`g07.045`](../roadmaps/g07/045-codegraph-parity-closeout.md)

Status: Paused
Owner: Platform
Created: 2026-05-18

## Purpose

Make `effigy graph` comparable to CodeGraph for day-to-day agent navigation
without adopting CodeGraph's JavaScript runtime, MCP-first integration, daemon
shape, or marketing claims.

The target is not feature mimicry. The target is the same practical outcome:
agents should answer architecture, ownership, flow, and change-impact questions
with sharply fewer filesystem calls, fewer file rereads, and bounded local
state.

## Lane Posture

Posture: `strict-parked`

The first `g07` graph work proved the native substrate: storage, freshness,
watch mode, extractors, query commands, `context`, and `explore`. The follow-up
assessment shows parity gaps:

- ranking still depends too much on in-process source reads
- `explore` does not traverse enough call/import/doc topology
- language support is much narrower than CodeGraph
- web route and framework entrypoints are not first-class graph facts
- no `affected` command exists for changed-file test targeting
- benchmark evidence is ad hoc rather than a repeatable harness
- agent instructions are good but not yet backed by a zero-reread contract

## Hard Boundaries

- no MCP server as graph-specific scope
- no graph daemon
- no JavaScript runtime dependency
- no external language plugin/package model
- no remote inference
- no embeddings
- no stored LLM-generated summaries as graph truth
- no unmeasured "X% faster" claim
- no breaking existing graph JSON contracts without a versioned successor

## Execution Order

1. `985` complete: open the parity lane and benchmark harness
2. `986` complete: index source-body evidence through SQLite FTS
3. `987` complete: add traversal-aware `explore` expansion
4. `988` complete: prioritize language extractor coverage
5. `989` complete: add framework routes and entrypoint edges
6. `990` complete: harden no-reread source-section packets
7. `991` complete: add affected-test and impact workflow
8. `992` complete: harden large-repo scale and storage migration
9. `993` complete: polish agent adoption and CLI workflow
10. `994` complete: run parity closeout and decide residual work
11. `995` complete: close or re-scope the generation front doors

## Ready Chain

- `985` is complete.
- `986` is complete.
- `987` is complete.
- `988` is complete.
- `989` is complete.
- `990` is complete.
- `991` is complete.
- `992` is complete.
- `993` is complete.
- `994` is complete.
- `995` is complete.
- no active ready card remains in this lane.

## Auto-Continuation Envelope

Auto-start is enabled while:

- each card closes with tests or a benchmark log
- each user-facing graph payload is versioned or additive
- benchmark claims stay evidence-based
- new extractors remain first-party Rust code compiled into Effigy
- graph output stays deterministic for a fixed index

Stop and replan if:

- useful parity requires MCP-only behavior
- useful parity requires generated semantic summaries as stored truth
- FTS or traversal changes make `graph explore` slower than direct `rg` for
  common warm-index navigation tasks
- language coverage would require a runtime dependency outside the Rust binary
- JSON payload size grows enough to become worse than direct file reads

## Acceptance

This lane is complete when:

- `effigy graph explore` has repeatable benchmark evidence against the
  CodeGraph-style workflow claims
- task-shaped architecture questions usually need zero immediate file rereads
  from returned owners
- exact-match work still routes agents to `rg`
- source-body matching is indexed rather than done by broad per-query reads
- traversal-aware results expose why related files were selected
- route/entrypoint facts exist for supported framework families
- affected-test targeting exists for changed files
- docs, skill, rustdoc, and JSON examples teach the final workflow honestly

## Next Task

This lane is paused. Continue in [`092`](./092-codegraph-parity-follow-up-strict-lane.md).
