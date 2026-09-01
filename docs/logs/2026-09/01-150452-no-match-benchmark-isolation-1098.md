# No-Match Benchmark Isolation 1098 Closeout

Status: complete
Created: 2026-09-01
Roadmap: g08.043
Batch: 1098-isolate-no-match-benchmark-from-live-corpus
Handoff: `20260901-144054-no-match-benchmark-1098.md`
Papercut: A no-match benchmark case cannot name itself in its own corpus

## Summary

- Empty-result benchmark proof now lives only on the fixture corpus. The live
  Effigy target keeps its five authority and historical cases and no longer
  owns a query whose correctness depends on terms remaining absent from live
  profile roots.
- The matrix rejects a live-target empty case before indexing or query
  execution. The fixture case `generic-no-match` (`quokka marmalade trombone`)
  stays non-vacuous and green even though those terms already appear in the
  historical card `1090` evidence log, which is a live profile root.
- Current matrix is 11 cases (6 fixture, 5 live). Historical freeze logs keep
  the older 12-case counts as historical evidence.
- No ranking, weighting, traversal, budgeting, profile, graph-store, refresh,
  CLI, or JSON behavior changed.

## Review oracle → proof

1. A case with `expect: "empty"` and the live repo as its target reaches query
   execution — falsified by `reject_live_empty_cases(repo_root, targets)`
   before `run_graph_index` / `evaluate_case`, and by
   `docs_context_benchmark_rejects_a_live_target_empty_case_before_query_execution`
   (throws on `live-empty` without `reached query execution`).
2. Removing the live empty case also removes the only fixture empty proof or
   makes that proof depend on live profile roots — falsified by committed
   `generic-no-match` remaining the sole `expect: "empty"` case, fixture query
   unchanged, and the live run returning 0 results / 0 context bytes while
   those terms remain in `docs/logs/2026-08/31-213000-northstar-profile-proof-1090.md`.
3. An unrelated live authority or historical case changes query, expected
   source, rival, rank bound, dimension, or pass criterion — falsified by
   `docs_context_benchmark_preserves_live_authority_and_historical_cases`
   (exact frozen fragments) and the 5/5 live benchmark pass.
4. The benchmark reports an old case total or describes the current matrix as
   the earlier freeze state — falsified by the sixth-freeze note and current
   11-case commentary in `scripts/benchmark-docs-context.rhai`, changelog, and
   guide `079`; historical 12/12 counts remain in freeze history and the card
   `1090` log.
5. The change adds a product exclusion option or touches retrieval runtime,
   profile grammar, graph storage, refresh, CLI, or JSON behavior — falsified
   by diff scope (`scripts/benchmark-docs-context.rhai`, focused
   `effigy-rhai` tests, guide/changelog, papercut/roadmap/card/log closeout).

## Empty-case inventory

- `generic-no-match` on `generic-handbook` — kept; `empty`; fixture-owned
- live `effigy-no-match` — removed from the current matrix
- live empty cases after validation — none; a synthetic live empty case fails
  matrix validation

## Unchanged live cases

`effigy-contract-authority`, `effigy-architecture-authority`,
`effigy-direct-historical-guide`, `effigy-next-task`, and
`effigy-historical-decision` keep their frozen queries, expected sources,
rivals, rank bounds, dimensions, and pass criteria.

## Changes

- `scripts/benchmark-docs-context.rhai`: remove live empty case; add
  `reject_live_empty_cases`; record sixth freeze and current 11-case matrix
- `crates/effigy-rhai/src/tests/docs_context_benchmark.rs`: focused
  matrix-guard, fixture-ownership, and live-case freeze proof
- Guide `079`, changelog, papercut, roadmap `g08.043`, card `1098`, and Next
  Task pointers closed back to publication planning

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`
- Movement: live empty-result proof coupled to corpus vocabulary → fixture-only
  empty proof with a structural live-empty rejection
- Remaining gap: None for this papercut; official catalog-pack publication
  planning under contract `043` remains the Next Task. Ranking/timeout
  papercuts stay separate.

## Validation Performed

- `cargo test -p effigy-rhai --lib docs_context_benchmark` — 5 passed
- `./target/debug/effigy perf:docs-context-benchmark` — 11/11 predeclared
  expectations held (`generic-no-match` 0 results; five live cases unchanged)
- `./target/debug/effigy qa:docs` — passed (links, JSON examples, index,
  forbidden, workflow-paths, vision index, vision next-action)
- `./target/debug/effigy qa` — 3643 passed, 1 skipped; docs and JSON-contract
  checks passed
- `cargo fmt --all -- --check` — passed
- `cargo clippy -p effigy-rhai --all-targets -- -D warnings` — passed
  (`proc-macro-error2` future-incompat notice only)
- `git diff --check` — passed

## Risks

- A future contributor could still document a *fixture* query inside the
  fixture corpus and turn that case red. That is the correct ownership: empty
  proof belongs to the fixture, not to Effigy's live docs.

## Next Task

- Return to planning for official catalog-pack publication and concrete-asset
  cutover under contract `043`. That lane needs a real OCI coordinate and
  explicit workflow-edit authority; it is not ready.
