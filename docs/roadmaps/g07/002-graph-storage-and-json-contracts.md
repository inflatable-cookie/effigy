# g07.002 - Graph Storage And JSON Contracts

Status: Planned
Depends on: `g07.001`

## Goal

Define the graph data model and stable JSON command contracts before writing
extractors.

This lane establishes the minimum fact vocabulary that every extractor and
query command must use.

## Scope

- choose the local storage backend, likely SQLite under `.effigy/graph/`
- define records for:
  - indexed files
  - symbols
  - edges
  - references
  - snippets
  - diagnostics
  - index runs
  - extractor versions
- define stable IDs for files, symbols, and edges
- define confidence/provenance fields
- define JSON schemas for graph command output
- add fixture-based contract tests

## Storage Guidance

The DB schema may evolve internally. Public stability belongs to CLI JSON.

Recommended DB tables:

- `graph_files`
- `graph_symbols`
- `graph_edges`
- `graph_references`
- `graph_diagnostics`
- `graph_index_runs`
- `graph_extractors`
- `graph_fts`

Use FTS only for search acceleration. Do not make FTS rows the canonical source
of graph facts.

## JSON Contract Requirements

Every response should include:

- `schema`
- `repo_root`
- `graph_version`
- `index_state`
- command-specific payload
- diagnostics where relevant

Every graph fact should include:

- stable id
- repo-relative path
- byte range where available
- line/column range where available
- extractor id/version
- confidence: `exact`, `syntactic`, `heuristic`, or `unknown`
- provenance text short enough for machine display

## Non-Goals

- no extractor implementation in this lane
- no query ranking work
- no DB migration framework beyond what v1 needs
- no remote or shared graph store

## Tests

- fixture DB round-trip tests
- JSON schema snapshot tests
- invalid record rejection tests
- stable ID tests across repeated indexing of the same fixture

## Acceptance Criteria

- storage schema can represent all planned v1 extractors
- JSON contracts are strict and versioned
- graph facts cannot be inserted without provenance and source ownership
- DB details remain hidden behind crate APIs

## Next Task

Plan and implement `g07.003`.
