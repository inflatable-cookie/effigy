# FTS-Backed Source Evidence

Date: 2026-05-18  
Roadmap: [`g07.037`](../../roadmaps/g07/037-fts-backed-source-evidence-and-ranking.md)  
Batch card: [`986`](../../roadmaps/g07/batch-cards/986-implement-fts-backed-source-evidence.md)  
Strict lane: [`091`](../../specs/091-codegraph-parity-strict-lane.md)

## What Changed

- indexed source-body text into the existing SQLite FTS store under an internal
  `source` record type
- kept public `graph search` stable by filtering internal source rows out of
  user-facing search results
- replaced broad per-query source reads in `graph context` ranking with indexed
  token-to-file lookups
- limited fallback source reads to selected owner files when a snippet span is
  still needed
- added regression coverage that source rows exist for ranking but do not leak
  into the public search contract

## Storage Shape

The storage change is additive.

- existing `graph_search` stays the FTS table
- new source-body entries use `record_type = "source"`
- public search still returns only file, symbol, and diagnostic rows
- next `graph index` refresh populates source rows; no schema rewrite or manual
  DB migration path is required for this slice

This keeps the hot ranking path indexed without introducing a new DB artifact
or breaking the public search payload.

## Validation

- `cargo test -p effigy-codegraph`
- `cargo build --bin effigy`

New regression:

- `graph_store_source_search_indexes_file_bodies_without_leaking_into_public_search`

## Warm Corpus After Reindex

The active corpus from
[`codegraph-parity-gold-queries.toml`](../../roadmaps/g07/codegraph-parity-gold-queries.toml)
was rerun against the live Effigy repo after `target/debug/effigy graph index`.

| Case | Query | Current top owner | Time | Delta vs `985` baseline |
| --- | --- | --- | ---: | --- |
| ownership | `trace deploy provider export` | `src/runner/deploy_command/provider_package.rs` | `1.63s` | improved timing; owner still correct |
| call-flow | `trace graph watch implementation` | `crates/effigy-codegraph/src/watch.rs` | `1.61s` | improved timing; owner still correct |
| architecture | `understand release orchestration` | `crates/effigy-cli/src/command_parsing_release.rs` | `1.62s` | timing improved; owner still acceptable alternate, but release library is not yet top |
| freshness | `find graph status stale detection` | `crates/effigy-codegraph/src/index.rs` | `1.71s` | improved timing; owner still correct |
| route proxy | `where are task routes parsed` | `src/runner/execute/routing.rs` | `1.77s` | owner improved back to expected primary after reindex |
| manifest | `what changes when a bundle source is git` | `crates/effigy-manifest/src/bundles/source.rs` | `1.71s` | improved timing; owner still correct |
| docs | `docs for graph agent workflow` | `docs/guides/076-code-graph-and-agent-workflows.md` | `1.70s` | improved timing; docs owner still correct |

## Interpretation

- this slice achieved the main architectural goal: ranking now uses indexed
  source evidence instead of broad candidate-file reads
- the live corpus did not regress owner quality on the active non-fixture cases
- route proxy and freshness cases stayed strong after reindex
- release architecture remains the main obvious ranking weakness in the active
  corpus; that is better handled by traversal-aware explore in `987` than by
  piling on more local heuristics here

## Residual Limits

- source evidence is still token-based, not semantic
- selected-file span recovery still reads the chosen owner file when no symbol
  span exists
- source-body indexing is additive inside the current FTS table, not yet tuned
  for large-repo storage pressure
- no fixture-backed affected-test or cross-language parity execution exists yet

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `CONTRACT`, `MAINT`
- moved: source-body evidence is now indexed and consumed by ranking through
  SQLite FTS rather than broad per-query file reads
- remains open: traversal-aware explore assembly, richer route and
  cross-language graph facts, and parity closeout

## Next Task

Execute `987`.
