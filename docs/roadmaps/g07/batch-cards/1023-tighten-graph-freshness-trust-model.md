# 1023 - Tighten Graph Freshness Trust Model

Roadmap: [`../073-graph-freshness-trust-and-cross-repo-readiness.md`](../073-graph-freshness-trust-and-cross-repo-readiness.md)
Strict lane: [`../../../specs/096-graph-agent-adoption-follow-through-strict-lane.md`](../../../specs/096-graph-agent-adoption-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-20

## Purpose

Make graph freshness clear enough for agents to decide whether to trust graph
results without parsing large stale-path lists.

## Work

- inspect current status and explore freshness payloads
- add compact trust state if needed
- keep detailed path diagnostics available
- test missing, stale, fresh, and failed-index cases
- verify behavior on cross-repo fixtures

## Guardrails

- no hidden auto-indexing in read-only commands
- no suppression of stale information
- no Effigy-specific ignore policy

## Acceptance

- freshness/trust state is compact and machine-readable
- detailed diagnostics remain available
- focused tests cover state transitions

## Next Task

Move to `1024`.
