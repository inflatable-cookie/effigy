# g07.035 - CodeGraph Parity Suite

Status: Active
Depends on: `g07.001` through `g07.034`

## Goal

Bring `effigy graph` up to the practical usefulness level of CodeGraph for
agent navigation while preserving Effigy's operating model: Rust binary, CLI
protocol, local deterministic graph state, no MCP requirement, no daemon, no
JavaScript runtime dependency.

## Why This Exists

CodeGraph's published win is not just "has a graph". It combines:

- a high-level exploration tool agents are instructed to trust
- FTS-backed search over indexed graph/source text
- call/import/reference traversal
- broad language coverage
- framework-aware routes and entrypoints
- native file watching and incremental sync
- enough source sections that agents avoid immediate rereads
- repeatable benchmark framing around tool calls, file reads, tokens, and time

Effigy now has the core substrate and a first `graph explore` command. The
remaining work is to remove the weak spots that still make an agent fall back
to broad `rg`, file reads, or manual graph stitching.

## Scope

- build a repeatable parity benchmark harness
- index source-body evidence through SQLite FTS
- make `explore` traversal-aware across calls, imports, docs, manifests, and
  route facts
- expand language extractor coverage in priority order
- add framework route and entrypoint graph facts
- improve source-section packets so agents can avoid rereading returned files
- add affected-test and changed-file impact workflow
- harden scale, storage migrations, and query latency for larger repos
- polish CLI, docs, skill, rustdoc, and JSON examples around the final agent
  workflow

## Non-Goals

- no MCP server
- no graph daemon
- no external language plugins
- no JavaScript runtime dependency
- no embeddings
- no remote inference
- no stored LLM-generated summaries as canonical graph data
- no broad "faster than CodeGraph" claim without benchmark proof

## Ordered Follow-Up Lanes

1. [`036-parity-benchmark-harness-and-claim-discipline.md`](./036-parity-benchmark-harness-and-claim-discipline.md)
2. [`037-fts-backed-source-evidence-and-ranking.md`](./037-fts-backed-source-evidence-and-ranking.md)
3. [`038-traversal-aware-explore-assembly.md`](./038-traversal-aware-explore-assembly.md)
4. [`039-richer-language-extractor-coverage.md`](./039-richer-language-extractor-coverage.md)
5. [`040-framework-route-and-entrypoint-edges.md`](./040-framework-route-and-entrypoint-edges.md)
6. [`041-source-section-packets-and-no-reread-workflow.md`](./041-source-section-packets-and-no-reread-workflow.md)
7. [`042-affected-test-and-impact-workflow.md`](./042-affected-test-and-impact-workflow.md)
8. [`043-large-repo-scale-and-storage-hardening.md`](./043-large-repo-scale-and-storage-hardening.md)
9. [`044-agent-adoption-and-cli-workflow-polish.md`](./044-agent-adoption-and-cli-workflow-polish.md)
10. [`045-codegraph-parity-closeout.md`](./045-codegraph-parity-closeout.md)

## Acceptance Criteria

- every parity gap is either closed, measured as acceptable, or recorded as a
  deliberate non-goal
- the benchmark harness can rerun without fresh planning judgment
- `graph explore` is usually the first and only broad-navigation call for
  task-shaped architecture questions
- docs and skill tell agents exactly when to trust graph output and when to use
  `rg`
- the final closeout does not overclaim

## Batch Cards

- [`985-open-codegraph-parity-benchmark-lane.md`](./batch-cards/985-open-codegraph-parity-benchmark-lane.md)
- [`986-implement-fts-backed-source-evidence.md`](./batch-cards/986-implement-fts-backed-source-evidence.md)
- [`987-implement-traversal-aware-explore.md`](./batch-cards/987-implement-traversal-aware-explore.md)
- [`988-expand-language-extractor-priority-set.md`](./batch-cards/988-expand-language-extractor-priority-set.md)
- [`989-add-framework-route-entrypoint-edges.md`](./batch-cards/989-add-framework-route-entrypoint-edges.md)
- [`990-harden-source-section-no-reread-packets.md`](./batch-cards/990-harden-source-section-no-reread-packets.md)
- [`991-add-affected-test-impact-workflow.md`](./batch-cards/991-add-affected-test-impact-workflow.md)
- [`992-harden-large-repo-scale-and-storage.md`](./batch-cards/992-harden-large-repo-scale-and-storage.md)
- [`993-polish-agent-adoption-and-cli-workflow.md`](./batch-cards/993-polish-agent-adoption-and-cli-workflow.md)
- [`994-run-codegraph-parity-closeout.md`](./batch-cards/994-run-codegraph-parity-closeout.md)
- [`995-close-or-rescope-codegraph-parity-lane.md`](./batch-cards/995-close-or-rescope-codegraph-parity-lane.md)

## Next Task

Execute `985`.
