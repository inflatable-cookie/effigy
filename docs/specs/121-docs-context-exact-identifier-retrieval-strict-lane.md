# 121 Docs Context Exact Identifier Retrieval Strict Lane

Status: Active
Owner: Effigy orchestrator
Created: 2026-09-05
Roadmap: [`g09.007`](../roadmaps/g09/007-docs-context-exact-identifier-retrieval.md)
Ready card: [`1114`](../roadmaps/g09/batch-cards/1114-docs-context-exact-identifier-retrieval.md)
Contract: [`041`](../contracts/041-documentation-graph-profile-contract.md)
Guide: [`079`](../guides/079-documentation-graph-profiles-and-context.md)

## Outcome

Exact identifiers in a `docs context` query seed and rank the sections that
literally contain them, with a machine-readable match reason, without
changing any other ranking, budget, or freshness behaviour.

## Fixed Decisions

- Identifier detection is repository-neutral: a raw query token that contains
  `_`, `-`, `.`, `::`, or `/` between alphanumeric runs is an identifier
  term. Plain words are unchanged.
- The identifier is retained whole as an exact term alongside its split
  words. Ranking credits exact whole-term containment (the existing
  `contains_term` boundary rule) in section text, heading, path, or field
  with a weight that outranks split-word density from documents lacking the
  identifier. The match reason names the exact term.
- Candidate recall may query the shared FTS index with the split words as a
  phrase or as separate tokens; no FTS tokenizer change, no second index, no
  storage schema migration.
- Two benchmark cases are added and frozen: an Effigy case for
  `catalog_tasks` expecting guide `026` at rank ≤ 3, and a generic-fixture
  identifier case with a rival document dense in the split words. The
  freeze is recorded in the script history. Existing cases and rank bounds
  are untouched.
- Spec `120` budgets still hold; a warm Effigy query succeeds under
  `EFFIGY_GRAPH_TIMEOUT_MS=5000`.
- Not allowed: stemming, fuzzy or prefix matching, synonyms, embeddings,
  changes to traversal, currentness, authority, budgets, freshness, locking,
  code-graph symbol search, or the JSON schema id.

## Dependency Runway

```text
card 1113 merged (latency budgets hold)
  -> 1114 exact identifier term + weight + two frozen benchmark cases
  -> exact-head review and merge
  -> Chatterbox resumes g09.006 freeze with the operator
```

One worker owns card `1114`. The change is small and well-located in
`docs_context/rank.rs`; use an economical non-frontier day-to-day worker.
Material review remains with the orchestrator.

## Whole-Lane Review Oracle

Reject the lane if any counterexample survives:

1. `docs context "catalog_tasks" --max-sections 3` on Effigy does not return
   the guide `026` section containing the literal, or its match reason does
   not name the exact term.
2. The generic-fixture identifier case is missing, unfrozen, or its rival
   outranks the exact match.
3. Any of the eleven pre-existing benchmark cases changes rank or fails.
4. `graph` matches `graphql`, or an identifier matches a longer identifier
   that merely contains it, as an exact term.
5. A warm Effigy query fails under 5000 ms after the change.
6. An FTS tokenizer change, storage migration, second index, stemming, fuzzy
   match, or schema id change appears.
7. Traversal, currentness, authority, budget, freshness, or lock behaviour
   changes.

Smallest counterexample set: the `catalog_tasks` query on Effigy; the new
fixture identifier query; one existing plain-word benchmark case; one
`graph`/`graphql` boundary test; one warm timing.

## Validation And Evidence

Card `1114` maps every oracle row to named proof. Run
`effigy perf:docs-context-benchmark` with the new freeze, focused
`docs_context` tests, one warm timing on Effigy, `effigy qa`,
`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
and `git diff --check`. Write one dated evidence log.

## Stop Conditions

Stop and return to the orchestrator if the fix needs an FTS tokenizer or
storage change, a second index, a ranking rule that alters existing benchmark
ranks, a budget or freshness change, or a contract `041` semantics change
beyond making retrieval rule 2 true for identifiers.

## Next Task

Execute card `1114`.
