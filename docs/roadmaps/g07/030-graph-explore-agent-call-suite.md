# g07.030 - Graph Explore Agent-Call Suite

Status: Complete
Depends on: `g07.001` through `g07.029`

## Goal

Add a high-level `effigy graph explore "<question>"` surface that can answer the
first agent navigation question in one command.

`graph context` currently narrows the file set. That is useful, but it still
usually forces an agent to open several files and run local `rg` before it can
form a working map. `graph explore` should return enough assembled, attributed
code context for an agent to start editing or reviewing with fewer follow-up
filesystem calls.

## Why This Exists

The CodeGraph comparison claims are measured at the whole-agent workflow level:
tool calls, file reads, and time to useful answer. The important shape is not
MCP by itself. The important shape is one high-level tool that:

- receives a task-shaped question
- returns ranked files and symbols
- includes enough code excerpts to avoid immediate rereads
- explains why each excerpt was selected
- keeps the index warm enough that the first call is cheap
- gives agents clear rules for when to trust the returned context

Effigy already has most of the substrate: indexing, watch mode, ranked context,
search snippets, callers/callees, impact, JSON output, and agent guidance. The
missing piece is the single-call exploration assembly and benchmark contract.

## Scope

- define an `explore` JSON contract that is richer than `context`
- establish whole-agent baseline metrics before implementation
- implement `effigy graph explore "<question>"`
- assemble ranked excerpts with reasons, ranges, and source provenance
- include related symbols/files without overloading the response
- document agent usage rules in docs, guides, rustdoc, and skill surfaces
- close with a workflow benchmark against the current `context -> read -> rg`
  pattern

## Non-Goals

- no MCP server
- no graph daemon
- no embeddings or LLM-generated canonical summaries
- no remote inference
- no replacement claim for exact `rg`
- no hidden global file reads outside indexed facts and selected excerpt reads
- no editor-specific integration

## Hard Boundaries

- `graph context` and existing graph JSON contracts must keep working
- `graph explore` may add a new schema payload, but must not break existing
  consumers
- every excerpt must include path, range, and selection reason
- output must remain bounded and deterministic for a fixed index
- generated summaries are not canonical graph facts; if added later, they must
  be explicit derived presentation, not stored truth
- exact-token misses must still point agents back to `rg`

## Ordered Follow-Up Lanes

1. [`031-explore-contract-and-benchmark-baseline.md`](./031-explore-contract-and-benchmark-baseline.md)
2. [`032-explore-context-assembly-command.md`](./032-explore-context-assembly-command.md)
3. [`033-agent-guidance-and-skill-update.md`](./033-agent-guidance-and-skill-update.md)
4. [`034-explore-benchmark-closeout.md`](./034-explore-benchmark-closeout.md)

## Acceptance Criteria

- a task-shaped query can return a useful owner map and excerpts in one command
- top-ranked excerpts are enough for normal first-pass agent understanding
- agents have clear instructions to use `explore` first and avoid rereading
  returned files unless the excerpt is insufficient
- benchmark evidence reports tool-call and file-read deltas honestly
- docs preserve the rule that `rg` remains the exact-match tool

## Batch Cards

- [`980-open-graph-explore-lane.md`](./batch-cards/980-open-graph-explore-lane.md)
- [`981-baseline-codegraph-style-agent-workflow.md`](./batch-cards/981-baseline-codegraph-style-agent-workflow.md)
- [`982-implement-graph-explore-command.md`](./batch-cards/982-implement-graph-explore-command.md)
- [`983-update-agent-guidance-and-docs.md`](./batch-cards/983-update-agent-guidance-and-docs.md)
- [`984-close-explore-benchmark-proof.md`](./batch-cards/984-close-explore-benchmark-proof.md)

## Next Task

No active graph explore task remains.
