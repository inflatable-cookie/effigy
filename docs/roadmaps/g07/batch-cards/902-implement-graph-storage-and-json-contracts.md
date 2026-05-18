# 902 - Implement Graph Storage And JSON Contracts

Roadmap: [`../002-graph-storage-and-json-contracts.md`](../002-graph-storage-and-json-contracts.md)
Strict lane: [`../../../specs/085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Create the graph storage owner and strict JSON contract types.

## Scope

- create the graph domain crate/module selected by `901`
- implement versioned storage initialization under `.effigy/graph/`
- model files, symbols, edges, references, diagnostics, index runs, and
  extractors
- add schema/version fields to JSON response types
- validate graph facts before storage
- add fixture tests for IDs, provenance, and round trips

## Guardrails

- DB schema is not the public contract
- extractors do not write storage directly
- graph facts without provenance are invalid
- do not implement query ranking yet

## Acceptance

- graph storage can be initialized and reopened
- graph records round-trip through storage APIs
- JSON response types are versioned and test-covered
- `903` can build index/status on top of the storage API

## What Landed

- added the new first-party crate:
  `crates/effigy-codegraph`
- added versioned graph artifact path ownership under:
  `.effigy/graph/graph.db`
- implemented storage bootstrap with:
  - metadata table
  - file, symbol, edge, reference, diagnostic, extractor, and index-run tables
  - FTS5-backed `graph_search` virtual table
- modeled and validated:
  - `GraphId`
  - `ExtractorId`
  - provenance
  - source spans
  - file/symbol/edge/reference/diagnostic/extractor/index-run records
- added typed JSON payload owners for:
  - status
  - files
  - search
  - node
  - callers/callees
  - impact
  - context
  - index-run history
- added crate tests covering:
  - id validation
  - storage initialization and reopen
  - record round trips and counts
  - provenance validation
  - JSON schema/version fields

## Notes

- the DB layout stays private; public contract work is in the typed JSON payloads
- extractor integration and command wiring are intentionally deferred to `903`
  and `904`
- grammar crates are still deferred; `902` only lands storage and contract
  substrate

## Next Task

Execute `903`.
