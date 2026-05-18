# g07.031 - Explore Contract And Benchmark Baseline

Status: Complete
Depends on: `g07.030`

## Goal

Define the `graph explore` contract and capture baseline workflow cost before
implementation changes.

## Scope

- define the text and JSON shape for `graph explore`
- choose 5 to 8 benchmark tasks from real Effigy navigation work
- measure the current workflow:
  - `effigy graph context`
  - file opens from the returned set
  - follow-up `rg`
  - total tool calls and file reads
- record where `context` already performs well
- record where the agent still needs too many follow-up reads
- identify minimum excerpt and related-symbol data needed to stop those reads

## Candidate Benchmark Tasks

- `trace deploy provider export`
- `trace graph watch implementation`
- `understand release orchestration`
- `find graph status stale detection`
- `docs for graph agent workflow`
- `where are task routes parsed`
- `what changes when a bundle source is git`

## Proposed JSON Payload

`graph explore --json` should return:

- `query`: normalized query text
- `index`: graph path, freshness state, indexed file count, indexed revision if
  available
- `summary`: deterministic text overview assembled from graph facts
- `primary`: ranked files/symbols that likely own the behavior
- `excerpts`: bounded code/doc excerpts with path, line range, language, role,
  score, reason, and excerpt text
- `relations`: callers, callees, docs, tests, and config neighbors where known
- `overflow`: useful files omitted because of size or limit constraints
- `guidance`: exact-match fallback note and any freshness warning

## Guardrails

- do not store generated prose as graph data
- do not read arbitrary full files just to make output look richer
- keep excerpt limits configurable but conservative
- preserve deterministic ordering for equal scores
- make the baseline fail in a useful way before tuning implementation

## Acceptance Criteria

- contract sketch is documented before code lands
- baseline log records current tool calls, file reads, and timings
- tests or fixtures are planned around stable expectations, not subjective
  hand-scoring
- `982` has a precise implementation target

## Evidence

- [`2026-05/18-132133-graph-explore-baseline.md`](../../logs/2026-05/18-132133-graph-explore-baseline.md)

## Next Task

Execute `982`.
