# g07.058 - Codegraph Manifest Query Module Decomposition

Status: Complete
Depends on: `g07.057`

## Goal

Split the largest graph modules into owned pieces so graph work remains
readable as query quality and language coverage grow.

## Evidence

The audit reported god-file findings for:

- `crates/effigy-codegraph/src/language/manifest/mod.rs`
- `crates/effigy-codegraph/src/query/mod.rs`
- `crates/effigy-codegraph/src/tests.rs`

`manifest/mod.rs` and `query/mod.rs` are the primary production risks.

## Scope

- split manifest extraction by fact family or manifest section
- split query assembly by concern:
  - ranking
  - source evidence / FTS
  - traversal
  - packet assembly
  - JSON-facing response shaping
- keep storage schema unchanged
- keep CLI output unchanged except for bug fixes with explicit tests
- move tests into smaller modules only where it improves failure locality

## Guardrails

- no query-ranking rewrite
- no database schema migration unless a small preparatory type move requires it
  and all callers remain compatible
- no benchmark claim changes
- no public JSON shape changes
- no broad formatter-only churn

## Suggested Implementation Shape

- extract manifest helper modules under `language/manifest/`
- extract query modules under `query/`
- leave a thin `query.rs` or `query/mod.rs` facade with the public API
- keep snapshots/gold fixtures stable
- run graph query tests before and after each major move

## Acceptance Criteria

- `manifest/mod.rs` and `query/mod.rs` are no longer god-file scan findings, or any
  remaining finding is explicitly justified
- all graph query and extractor tests pass
- no graph CLI JSON contract changes are introduced accidentally
- module names explain the graph concern they own

## Next Task

No active ready card.
