# g08.043 Docs Context No-Match Benchmark Isolation Papercut

Status: Complete
Created: 2026-09-01
Completed: 2026-09-01
Card: [`1098`](./batch-cards/1098-isolate-no-match-benchmark-from-live-corpus.md)
Contract: [`041`](../../contracts/041-documentation-graph-profile-contract.md)
Papercut: [`PAPERCUTS.md`](../../../PAPERCUTS.md)
Evidence: [`../../logs/2026-09/01-150452-no-match-benchmark-isolation-1098.md`](../../logs/2026-09/01-150452-no-match-benchmark-isolation-1098.md)

## Purpose

Make the documentation-context no-match proof independent of prose written in
Effigy's live documentation corpus.

## Decision

- Empty-result benchmark cases run only against dedicated fixture corpora.
- The live Effigy target keeps its authority and historical-retrieval cases,
  but owns no query whose correctness depends on terms remaining absent from
  live profile roots.
- The benchmark rejects a future empty-result case pointed at the live repo
  before executing the matrix.
- Historical freeze evidence stays historical; current matrix commentary and
  result counts describe the new fixture-only shape honestly.

## Scope

- `scripts/benchmark-docs-context.rhai`
- focused benchmark recurrence proof
- papercut, roadmap, evidence, and active-pointer closeout

## Boundary

- no documentation-context ranking, weighting, traversal, budgeting, profile,
  extractor, graph-store, refresh, CLI, or JSON change
- no new corpus-exclusion feature in the product runtime
- no timeout, catalog-pack publication, release/workflow, S3, or rollover work

## Cards

- [x] [`1098`](./batch-cards/1098-isolate-no-match-benchmark-from-live-corpus.md) — complete

## Acceptance

- every empty-result case is rooted in a fixture corpus, not the live repo
- a structural guard prevents a future live-corpus empty-result case
- the fixture no-match case remains green even when its purpose is documented
  inside Effigy's normal profile roots
- all remaining live authority and historical cases retain their frozen inputs
  and pass criteria
- benchmark, docs QA, and full repository validation pass

## Next Task

Return to official catalog-pack publication planning under contract `043`.
