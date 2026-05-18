# g07.037 - FTS-Backed Source Evidence And Ranking

Status: Complete
Depends on: `g07.036`

## Goal

Move source-body evidence out of broad per-query file reads and into indexed
SQLite FTS so `graph context` and `graph explore` can rank by source content
quickly and predictably.

## Scope

- extend graph storage with a versioned FTS source/body index
- index file path, symbol names, selected source text, diagnostics, doc
  headings, and manifest/task identifiers with explicit record types
- preserve existing `graph_search` consumers or migrate them through an
  additive store API
- expose a store query that returns:
  - file id
  - matched token coverage
  - rank
  - match span or best snippet seed when available
- update `context` ranking to consume indexed evidence instead of reading every
  candidate source file
- keep exact text verification delegated to `rg`
- add regression tests for:
  - docs/comments not outranking owner code on implementation queries
  - task-route language finding selector/routing owners
  - stale/status query finding graph freshness owners
  - source FTS migration from an old graph DB

## Guardrails

- do not store generated summaries as truth
- do not make FTS schema changes silently incompatible
- do not rank by raw repeated token frequency alone
- do not let comments/docs dominate implementation intent
- do not read every indexed file during a query after this lands

## Acceptance Criteria

- warm `graph explore` ranking no longer does broad source reads for evidence
- benchmark tasks show equal or better owner ranking than the current scorer
- query latency stays within the `g07.036` budget
- old graph DBs either migrate or rebuild with a clear error/remediation path

## Evidence

- [`2026-05/18-144300-fts-backed-source-evidence.md`](../logs/2026-05/18-144300-fts-backed-source-evidence.md)

## Next Task

Execute `987` after FTS-backed evidence is stable.
