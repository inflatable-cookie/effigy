# 986 - Implement FTS-Backed Source Evidence

Roadmap: [`../037-fts-backed-source-evidence-and-ranking.md`](../037-fts-backed-source-evidence-and-ranking.md)
Strict lane: [`../../../specs/091-codegraph-parity-strict-lane.md`](../../../specs/091-codegraph-parity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Move source-body ranking evidence into indexed SQLite FTS so query ranking is
fast, deterministic, and not driven by ad hoc file reads.

## Work

- extend storage with versioned source/body FTS records
- index selected source text and graph labels during `graph index`
- expose store APIs for token coverage, rank, and snippet seed lookup
- update `context` and `explore` ranking to use indexed evidence
- keep exact-match verification delegated to `rg`
- add migration and ranking regression tests
- rerun the gold queries from `985`

## Acceptance

- broad source reads are removed from ranking hot paths
- ranking quality does not regress against the `985` baseline
- storage migration/rebuild behavior is explicit
- benchmark evidence records latency and owner-ranking deltas

## Evidence

- [`2026-05/18-144300-fts-backed-source-evidence.md`](../../../logs/2026-05/18-144300-fts-backed-source-evidence.md)

## Next Task

Execute `987`.
