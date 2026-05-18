# 910 - Implement Graph Query Commands

Roadmap: [`../010-query-commands.md`](../010-query-commands.md)
Strict lane: [`../../../specs/085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Expose graph data through stable CLI JSON queries.

## Scope

- `effigy graph search <query> --json`
- `effigy graph files --json`
- `effigy graph node <id> --json`
- `effigy graph callers <id> --json`
- `effigy graph callees <id> --json`
- `effigy graph impact <path|symbol> --json`
- stale warning propagation
- result caps and deterministic ranking

## Guardrails

- no silent index rebuild during query
- no LLM reranker
- no server mode
- text output is secondary

## Acceptance

- agents can answer common navigation questions with CLI JSON only
- query output is bounded, provenance-backed, and schema-versioned
- stale graph state is visible in responses

## Next Task

Execute `906`.
