# 090 - Graph Explore Agent Navigation Strict Lane

Roadmap: [`g07.030`](../roadmaps/g07/030-graph-explore-agent-call-suite.md)
Related planning:
- [`g07.031`](../roadmaps/g07/031-explore-contract-and-benchmark-baseline.md)
- [`g07.032`](../roadmaps/g07/032-explore-context-assembly-command.md)
- [`g07.033`](../roadmaps/g07/033-agent-guidance-and-skill-update.md)
- [`g07.034`](../roadmaps/g07/034-explore-benchmark-closeout.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Add a high-level graph exploration command that reduces whole-agent navigation
cost for architecture and ownership questions.

## Lane Posture

Posture: `strict-closed`

This lane exists because the current graph surface is useful but still too
low-level for CodeGraph-style workflow gains. The expected improvement is not
raw query latency. It is fewer follow-up file reads after the first graph call.

## Hard Boundaries

- no MCP server
- no graph daemon
- no embeddings
- no remote inference
- no stored LLM-generated summaries as graph truth
- no breaking existing graph JSON contracts
- no exact-search replacement claim

## Execution Order

1. `980` complete: lane opened and ready chain wired
2. `981` complete: baseline CodeGraph-style agent workflow
3. `982` complete: implement graph explore command
4. `983` complete: update agent guidance and docs
5. `984` complete: close benchmark proof

## Ready Chain

- no ready card remains

## Auto-Continuation Envelope

Auto-start is enabled while:

- each batch closes with tests, docs, or a benchmark log
- existing graph commands remain compatible
- benchmark claims stay evidence-based
- implementation stays within graph query, CLI parsing, runner dispatch, JSON,
  docs, and tests

Stop and replan if:

- useful output requires generated semantic summaries as canonical data
- the command needs an MCP or daemon dependency
- JSON contract changes would break existing consumers
- benchmark evidence shows `explore` is only cosmetic

## Acceptance

This lane is complete when:

- `graph explore` has a stable CLI and JSON contract
- returned excerpts are useful enough to reduce first-pass file reads
- docs and skill teach the workflow clearly
- benchmark closeout records the actual win/loss profile

## Next Task

No active task remains in lane `090`.
