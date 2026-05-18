# g07.011 - Agent Context Packs

Status: Complete
Depends on: `g07.010`

## Goal

Add a bounded context-pack command for agents.

This is the feature that turns graph facts into a useful starting packet for a
coding task without scanning the whole repo.

## Scope

- `effigy graph context "<task>" --json`
- rank likely relevant files, symbols, docs, tasks, and manifests
- include bounded snippets with source ranges
- include why each item was selected
- include omitted/overflow counts
- include freshness warning
- support flags for max files, max bytes, language filters, and path filters

## Ranking Inputs

Start deterministic:

- lexical search over names/docs/snippets
- file/symbol graph neighborhood
- manifest/task ownership links
- docs links to code paths
- recent/stale file freshness where available

Do not use an LLM for v1 ranking. If LLM scoring is ever added, keep it outside
canonical graph data.

## Output Requirements

Every context item should include:

- kind
- path
- range where available
- score or rank
- selection reason
- provenance
- snippet if allowed

## Non-Goals

- no generated implementation plan
- no generated natural-language repo summary as canonical data
- no automatic edits
- no hidden file reads outside the graph/index policy

## Tests

- fixture task query selects expected files/docs
- max-byte cap enforcement
- stale index warning
- snippet truncation
- ranking reason snapshots

## Acceptance Criteria

- context output is small enough for agent prompts
- every selected item explains why it is present
- agents can use the output without direct broad scanning
- context packs remain deterministic for the same graph and query

## Next Task

Plan and implement `g07.012`.
