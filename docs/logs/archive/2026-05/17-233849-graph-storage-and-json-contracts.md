# Graph Storage And JSON Contracts

Date: 2026-05-17
Roadmap: `g07.001`
Card: `902`

## Summary

Landed the first native graph substrate in a dedicated crate.

This batch adds private storage ownership under `.effigy/graph/` and the first
typed JSON response surfaces without exposing any CLI commands yet.

## What Changed

- added `crates/effigy-codegraph`
- added storage bootstrap for `.effigy/graph/graph.db`
- added normalized record models for:
  - files
  - symbols
  - edges
  - references
  - diagnostics
  - extractors
  - index runs
- added validation rules for ids, spans, provenance, and unresolved/resolved
  edge/reference shape
- added FTS5 virtual table bootstrap for future search work
- added typed JSON payload owners for:
  - status
  - files
  - search
  - node
  - callers/callees
  - impact
  - context
  - index-run history
- added crate tests for reopen, round trips, counts, provenance rejection, and
  JSON schema/version fields

## Current State

- graph storage owner exists and reopens cleanly
- record round trips are test-covered
- JSON payloads are typed and versioned
- no command wiring has landed yet
- no extractor or repo-walk logic has landed yet

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved in this report:
  - graph storage substrate exists
  - graph JSON contract substrate exists
  - `903` can build index/status on top of a stable owner
- remains open:
  - graph index/status commands
  - freshness logic
  - extractor framework
  - language indexers
  - query and context commands

## Validation

Commands run:

```bash
cargo test -p effigy-codegraph
cargo fmt --all -- --check
```

## Next Task

Execute `903`.
