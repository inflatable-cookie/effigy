# 1114 - Docs Context Exact Identifier Retrieval

Roadmap: [`../007-docs-context-exact-identifier-retrieval.md`](../007-docs-context-exact-identifier-retrieval.md)
Spec: [`../../../specs/121-docs-context-exact-identifier-retrieval-strict-lane.md`](../../../specs/121-docs-context-exact-identifier-retrieval-strict-lane.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/041-documentation-graph-profile-contract.md`](../../../contracts/041-documentation-graph-profile-contract.md)

Status: Complete
Owner: docs-context query terms, lexical seeding and scoring, benchmark matrix
Created: 2026-09-05
Ready since: 2026-09-05 operator confirmation
Completed: 2026-09-05; PR `92`; reviewed implementation head `29831fc04`

## Purpose

Make an exact identifier in a query find the section that literally contains
it, and freeze that behaviour in the benchmark.

## Work

- in `crates/effigy-codegraph/src/docs_context/rank.rs`, extend
  `query_terms` (or its caller) so an identifier-shaped raw token is kept
  whole as an exact term alongside its split words
- credit exact whole-term containment of the identifier in section text,
  heading, path, and field values with a weight that outranks split-word
  density; add a match reason that names the exact term
- keep candidate recall working through the existing FTS `source_search`
  (split words or phrase); do not change the tokenizer or storage schema
- add one Effigy benchmark case (`catalog_tasks` → guide `026`, rank ≤ 3)
  and one generic-fixture identifier case with a split-word-dense rival
  (add a small fixture document if the corpus lacks one); record the freeze
  in the script history before running
- prove `graph`/`graphql` and identifier-prefix boundaries in unit tests
- update guide `079` (how identifiers are matched); add `CHANGELOG.md`
  `[Unreleased]` **Fixed** entry; write one evidence log

## Acceptance

- [x] `effigy docs context "catalog_tasks" --max-sections 3` on Effigy
      returns the guide `026` section containing the literal in the top 3
      with a match reason naming `catalog_tasks`
- [x] new fixture identifier case passes with its rival ranked below
- [x] all eleven pre-existing benchmark cases keep their ranks; freeze
      history updated
- [x] `graph` does not match `graphql`; the exact term does not match a
      longer identifier containing it
- [x] warm Effigy query succeeds under `EFFIGY_GRAPH_TIMEOUT_MS=5000`
- [x] no FTS tokenizer, storage, schema id, budget, freshness, traversal,
      currentness, or authority change

## Review Oracle

Falsify these counterexamples before PR creation:

1. `catalog_tasks` still misses guide `026` in the top 3.
2. Fixture rival outranks the exact match, or the case is not frozen.
3. An existing benchmark case changed rank.
4. A boundary test fails (`graph`/`graphql`, identifier prefix).
5. Warm 5000 ms query fails.
6. Tokenizer, storage, schema, budget, or freshness changed.

## Validation

- focused `docs_context` unit and integration tests
- `effigy perf:docs-context-benchmark`
- one warm timing on Effigy at 5000 ms
- `effigy graph affected` for changed source, then direct targets
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

One dated closeout log under `docs/logs/2026-09/` mapping every oracle row to
exact proof: the `catalog_tasks` result with rank and reason, the benchmark
table with the freeze commit, boundary tests, the warm timing, and validation.

## Stop Conditions

Stop if the fix needs an FTS tokenizer or storage change, a second index, a
ranking change that moves existing benchmark ranks, a budget or freshness
change, or a contract `041` semantics change beyond making retrieval rule 2
true for identifiers.

## Next Task

PR `92` closeout is on this branch. Independent exact-head review of the
closeout commit, then orchestrator merge. K4 is closed; K5 remains in
[`20260905-docs-context-identifier-retrieval-and-k5-expectation`](../../../triage/20260905-docs-context-identifier-retrieval-and-k5-expectation.md).
Evidence: [`05-133718`](../../../logs/2026-09/05-133718-docs-context-exact-identifier-1114.md).
