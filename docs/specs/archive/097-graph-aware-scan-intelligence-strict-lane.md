# 097 - Graph-Aware Scan Intelligence Strict Lane

Roadmap: [`g08.001`](../roadmaps/g08/001-graph-aware-scan-intelligence-suite.md)
Related planning:
- [`g08.002`](../roadmaps/g08/002-scan-graph-contract-and-readiness-model.md)
- [`g08.003`](../roadmaps/g08/003-existing-scan-graph-enrichment.md)
- [`g08.004`](../roadmaps/g08/004-boundary-and-layer-violation-scans.md)
- [`g08.005`](../roadmaps/g08/005-dead-and-isolated-code-scans.md)
- [`g08.006`](../roadmaps/g08/006-validation-gap-and-hotspot-scans.md)
- [`g08.007`](../roadmaps/g08/007-agent-docs-json-and-benchmark-proof.md)
- [`g08.008`](../roadmaps/g08/008-graph-aware-scan-closeout.md)

Status: Complete
Owner: Platform
Created: 2026-05-25

## Purpose

Make `effigy scan` able to use graph data where relationships matter, while
preserving the current filesystem-first scan contract.

The graph should add evidence about ownership, impact, boundaries, isolation,
and validation risk. It must not make ordinary scans slower, less predictable,
or unavailable when no index exists.

## Lane Posture

Posture: `complete`

This lane starts with contract work. Implementation must begin by defining how
scan commands detect, report, and skip graph data before adding new findings.

## Hard Boundaries

- no hidden graph indexing from scan commands
- no breaking rewrite of current scan JSON contracts
- no Effigy-only architecture rules
- no LLM-generated scan findings
- no graph daemon, MCP surface, or JavaScript runtime dependency
- no release mutation
- no `.github/workflows/` edits

## Cross-Repo Rule

Graph-aware scans must work as repo-agnostic tooling.

Effigy may be a regression target, but every rule must be designed around
generic graph facts, manifest config, or fixture-backed examples. Optional
live repos such as Underlay or decodelabs can add proof, but tests cannot
require private local repos.

## Execution Order

1. `1029`: open the lane and record the scan/graph baseline
2. `1030`: define scan graph readiness and JSON contract
3. `1031`: enrich existing scan findings with optional graph context
4. `1032`: add boundary and layer violation scans
5. `1033`: add dead and isolated code scans
6. `1034`: add validation-gap and hotspot scans
7. `1035`: update agent docs, examples, and benchmark proof
8. `1036`: close with proof and residual limits

## Ready Chain

- `1029` is complete
- `1030` is complete
- `1031` is complete
- `1032` is complete
- `1033` is complete
- `1034` is complete
- `1035` is complete
- `1036` is complete
- later cards must not start until the prior card is complete or explicitly
  paused with a clear handoff

## Stop Conditions

Stop and replan if:

- graph-aware scan behavior would require auto-indexing
- a finding cannot cite concrete graph evidence
- JSON changes would break current scan consumers
- a rule only works because of Effigy paths or crate names
- heuristic findings become noisy enough to undermine normal agent use

## Acceptance

This lane is complete when:

- all cards `1029` through `1036` are complete
- existing scans still work without a graph index
- graph-backed findings are tested, documented, and clearly marked
- cross-repo or fixture-backed proof is recorded
- no active ready card remains

## Next Task

No active ready card.
