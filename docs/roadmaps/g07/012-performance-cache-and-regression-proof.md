# g07.012 - Performance, Cache, And Regression Proof

Status: Complete
Depends on: `g07.010`, `g07.011`

## Goal

Prove the graph feature is fast, useful, and safe enough to ship.

This lane closes the suite with benchmark evidence, regression tests, and
accepted limitations.

## Scope

- benchmark indexing Effigy itself
- benchmark representative PHP and JS/TS fixture repos
- measure DB size
- measure query latency
- measure context-pack size and latency
- prove stale detection cost
- compare common graph queries against direct `rg` exploration
- document known accuracy limits
- update guides and agent skill guidance

## Metrics To Capture

- files scanned
- files indexed
- skipped files by reason
- symbol count
- edge count by confidence
- diagnostics count
- cold index duration
- no-op index duration
- query p50/p95 where practical
- DB size

## Regression Proof

Run:

- graph unit tests
- extractor fixture tests
- JSON contract tests
- docs path checks
- at least one full `effigy graph index` against Effigy itself
- targeted `graph context` checks

## Non-Goals

- no perfect accuracy claim
- no "all languages supported" claim
- no release until accepted limitations are explicit
- no shipping if graph commands are slower than direct exploration for common
  tiny queries without a clear explanation

## Acceptance Criteria

- closeout log records metrics and limits
- graph commands are predictable under stale and fresh states
- context packs are measurably smaller than broad file reads
- known gaps are documented as roadmap follow-ups
- suite can move from planned to shipped or explicitly split into follow-up
  roadmaps

## Next Task

Open a follow-up graph tranche only after reviewing the closeout limits.
