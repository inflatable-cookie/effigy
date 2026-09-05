# Docs Context Exact Identifier Retrieval 1114 Closeout

Status: complete
Created: 2026-09-05
Roadmap: [`g09.007`](../../roadmaps/g09/007-docs-context-exact-identifier-retrieval.md)
Spec: [`121`](../../specs/121-docs-context-exact-identifier-retrieval-strict-lane.md)
Batch: docs-context-exact-identifier-retrieval-1114
Contract: [`041`](../../contracts/041-documentation-graph-profile-contract.md)
PR: [`92`](https://github.com/inflatable-cookie/effigy/pull/92)
Reviewed implementation head: `29831fc04cfda6fc1848b1b3d7e915567a7759f5`
Freeze commit: `62dbe45121891a34047ca06da1376a387ec82e41`

## Summary

`effigy docs context "catalog_tasks"` now returns guide `026`'s containing
section at rank 1 with a match reason that names `catalog_tasks`. Identifier-
shaped query tokens stay whole alongside split words. Exact whole-term
containment outranks split-word density. FTS tokenizer, storage schema, budgets,
freshness, traversal, currentness, and authority are unchanged.

This closes the K4 exact-identifier defect recorded after card `1113`. K5
remains in
[`20260905-docs-context-identifier-retrieval-and-k5-expectation`](../../triage/20260905-docs-context-identifier-retrieval-and-k5-expectation.md).

## Catalog Tasks Result

Command: `./target/release/effigy --json docs context "catalog_tasks" --max-sections 3`

| Field | Value |
| --- | --- |
| Rank | 1 |
| Path | `docs/guides/026-json-payload-examples.md` |
| Heading | `2) Tasks (effigy.tasks.v1)` |
| Relevance | 69 |
| Terms | `catalog` (df 341, weighted), `tasks` (df 469, weighted), `catalog_tasks` (df 18, weighted) |

Match reasons: section text contains `catalog`; heading contains `tasks`;
section text contains `tasks`; section text contains `catalog_tasks`.

Ranks 2 and 3 were spec `121` and a traversed contract `041` hop. Guide `026`
is inside the top 3 with the exact-term reason.

## Benchmark Freeze

`effigy perf:docs-context-benchmark` after freeze `62dbe451`: 13/13 held.
Seventh freeze in `scripts/benchmark-docs-context.rhai`. Existing eleven cases
kept their rank bounds.

| Case | Rank | Rival rank | Result |
| --- | --- | --- | --- |
| `generic-charter-authority` | 1 | 2 | pass |
| `generic-current-over-retired` | 1 | 2 | pass |
| `generic-historical-direct` | 1 | — | pass |
| `generic-authority-gate` | — | — | pass |
| `generic-no-match` | — | — | pass |
| `generic-relation-follow-up` | 4 | — | pass |
| `generic-exact-identifier` | 1 | 2 | pass |
| `effigy-contract-authority` | 1 | — | pass |
| `effigy-architecture-authority` | 1 | — | pass |
| `effigy-direct-historical-guide` | 1 | — | pass |
| `effigy-next-task` | 1 | — | pass |
| `effigy-historical-decision` | 2 | — | pass |
| `effigy-exact-identifier` | 1 | — | pass |

Fixture case `generic-exact-identifier`: query `snorkel_grommet`, expected
`handbook/playbooks/coil-alignment.md` rank 1, rival
`handbook/playbooks/snorkel-station.md` rank 2.

## Warm Timing

Release binary, current index, `EFFIGY_GRAPH_TIMEOUT_MS=5000`,
`docs context "catalog_tasks" --max-sections 3`: 1662 ms, ok. Guide `026` still
rank 1.

Debug warm missed the bound (~5182 ms). The spec 120 budget is the timeout, not
a debug-profile target. Card `1113` measured the same query on release.

## Review Oracle

| Counterexample | Status |
| --- | --- |
| 1. `catalog_tasks` still misses guide `026` in the top 3 | falsified: rank 1, reason names `catalog_tasks` |
| 2. Fixture rival outranks the exact match, or the case is not frozen | falsified: rank 1 vs rival 2; seventh freeze at `62dbe451` |
| 3. An existing benchmark case changed rank | falsified: eleven pre-existing cases passed at prior rank bounds |
| 4. A boundary test fails (`graph`/`graphql`, identifier prefix) | falsified: `graph_does_not_match_graphql`; `exact_identifier_does_not_match_a_longer_identifier`; integration `exact_identifier_outranks_split_word_density` |
| 5. Warm 5000 ms query fails | falsified: 1662 ms on release |
| 6. Tokenizer, storage, schema, budget, or freshness changed | falsified: no edits to `storage.rs`, schema id, budgets, freshness, lock, traversal, currentness, or authority |

Independent review of exact head `29831fc04` accepted every behavioral oracle
row and check. Non-blocking notes (sentence punctuation, weight comment,
fixture construction, spec wording) are deferred; none invalidates an oracle.

## Validation

| Check | Result |
| --- | --- |
| focused `docs_context` tests | 31 passed, including the three identifier tests |
| `effigy perf:docs-context-benchmark` | 13/13 held |
| warm Effigy query at 5000 ms | 1662 ms, ok |
| `effigy graph affected` on changed source | `qa` selected; likely tests were heuristic noise outside `docs_context` |
| `effigy qa` | pass |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `git diff --check` | clean |

## Vision Target Delta

- Primary tags: `OPERATE`, `ROUTE`, `CONTRACT`, `MAINT`
- Movement: `docs context "catalog_tasks"` missed guide `026` in the top 32
  because the identifier split into common words → rank 1 with an exact-term
  match reason; contract `041` retrieval rule 2 now holds for identifiers
- Remaining gap: K5 rephrasing stays in the canonical triage note; `g09.006`
  is queued and out of this lane

## Next Task

Independent exact-head review of this closeout commit, then orchestrator merge
of PR `92`. Do not merge from the worker. Chatterbox resumes `g09.006` with the
operator after merge.
