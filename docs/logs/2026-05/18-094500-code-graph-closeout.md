# Code Graph Closeout

Date: 2026-05-18  
Roadmaps: [`g07.001`](../roadmaps/g07/001-code-graph-intelligence-suite.md),
[`g07.012`](../roadmaps/g07/012-performance-cache-and-regression-proof.md)  
Batch card: [`912`](../roadmaps/g07/batch-cards/912-close-code-graph-intelligence-proof.md)  
Strict lane: [`085`](../specs/085-code-graph-intelligence-strict-lane.md)

## What Changed

- closed the native code graph tranche with measured index, query, and context
  pack evidence
- updated command-reference docs so `effigy graph` is part of the active lookup
  surface
- updated the bundled Effigy agent skill with the graph-first discovery path
  for bounded repo navigation
- closed `g07.001`, `g07.012`, batch `912`, and strict lane `085`

## Metrics

### Effigy Repo

- cold full-repo index: `144.14s`
- no-op full-repo index: `148.39s`
- graph DB size: `138,022,912` bytes (`131.6 MiB`)
- indexed files: `3191`
- stored file records: `3184`
- symbols: `30,800`
- edges: `138,885`
- references: `62,856`
- diagnostics: `7`
- extractors: `5`
- failed paths: `7`

### Query Latency

- `graph status --json`: `2.34s`
- `graph search release --limit 5 --json`: `4.43s`
- `graph context "trace release orchestrator" --language rust --max-files 6 --max-bytes 2048 --json`: `2.73s`
- direct `rg -n "release orchestrator" src crates docs`: `0.05s`

### Context Pack Size

- context JSON artifact: `35,787` bytes
- bounded snippet budget: `2,048` bytes
- unique source files represented: `6`
- raw bytes across those source files: `382,474`

The graph context payload is materially smaller than reading the same source
files directly, even before prompt-side trimming, while carrying rank, reasons,
and provenance.

### Small Fixture Index Cost

- representative PHP fixture index: `0.19s`
  - `3` files, `9` symbols, `9` edges, `1` reference, `0` diagnostics
- representative JS/TS fixture index: `0.19s`
  - `3` files, `7` symbols, `9` edges, `2` references, `0` diagnostics

### Stale Detection

- stale `graph status --json` on a modified tiny fixture: `0.12s`
- stale path detection correctly reported `src/lib.rs` as both stale and
  changed

## Limits Accepted In This Closeout

- no-op indexing is still effectively full reindex cost; there is no useful
  incremental short path yet
- `graph search` is slower than direct `rg` for tiny lexical queries, but it
  returns typed symbol/file records rather than raw line hits
- the full Effigy repo index still reports seven failed manifest/export
  fixtures:
  - `crates/effigy-manifest/tests/fixtures/php-app-bundle/export.toml`
  - `crates/effigy-manifest/tests/fixtures/php-library-bundle/export.toml`
  - `crates/effigy-manifest/tests/fixtures/workspace-app-bundle/export.toml`
  - `examples/render-provider-smoke/effigy.toml`
  - `external/bundles/decodelabs-library/effigy.toml`
  - `external/bundles/decodelabs/effigy.toml`
  - `external/bundles/underlay/export.toml`
- context ranking is deterministic and useful, but still lexical/structural
  rather than semantic

## Validation

- `cargo test -p effigy-codegraph`
- `cargo test graph -- --nocapture`
- `cargo check -p effigy-cli -p effigy`
- `cargo fmt --all -- --check`
- `./target/debug/effigy docs check paths docs/roadmaps/g07 docs/roadmaps/README.md docs/roadmaps/generation-index.md docs/specs/085-code-graph-intelligence-strict-lane.md docs/specs/README.md docs/logs/README.md docs/logs/2026-05/18-082531-agent-context-packs.md docs/logs/2026-05/18-094500-code-graph-closeout.md docs/guides/025-command-reference-matrix.md`
- `git diff --check`

## Follow-Up Candidates

- incremental graph indexing keyed by content hash and extractor stability
- faster query projections or precomputed search/context summaries for large
  repos
- deeper manifest/export fixture support so the full Effigy repo indexes
  without failed-path gaps

## Vision Target Delta

- primary vision tags touched: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`
- moved: no graph surface -> first-party local graph indexing, typed graph
  queries, and bounded agent context packs with measured repo-scale behavior
- remains open: no active `g07` card; follow-up work is optional and should
  start from incremental indexing, query speed, and failed fixture-path depth
