# g07.010 - Query Commands

Status: Complete
Depends on: `g07.002`, `g07.003`, `g07.005`, `g07.006`, `g07.007`

## Goal

Expose the graph through stable CLI queries that agents can use directly.

## Scope

- `effigy graph search <query> --json`
- `effigy graph files --json`
- `effigy graph node <id> --json`
- `effigy graph callers <id> --json`
- `effigy graph callees <id> --json`
- `effigy graph impact <path|symbol> --json`
- text rendering for humans where useful
- stale-index warnings on every query response
- strict JSON contract tests

## Query Rules

- never silently rebuild during query unless an explicit flag says so
- show stale state clearly
- cap default result counts
- include paths, ranges, symbol kinds, confidence, and provenance
- prefer deterministic ranking over opaque scoring

## Impact Query Guidance

`impact` should start with graph-neighborhood evidence:

- file containment
- import/include neighbors
- direct caller/callee edges
- manifest/task/docs references
- affected tests where graph evidence supports it

Do not claim full blast radius. Label it as graph-derived impact.

## Non-Goals

- no natural-language search engine as v1 core
- no LLM reranker
- no server mode
- no editor protocol

## Tests

- query fixture DB
- search result ordering
- stale warning propagation
- callers/callees on exact and heuristic edges
- impact output shape
- JSON contract snapshots

## Acceptance Criteria

- agents can answer common navigation questions with CLI JSON only
- stale index state is visible in every relevant response
- query output is bounded and provenance-backed
- text output does not become the contract

## Next Task

Continue `g07.006` and `g07.011`.
