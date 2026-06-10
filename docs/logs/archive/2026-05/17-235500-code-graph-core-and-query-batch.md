# Code Graph Core And Query Batch

Date: 2026-05-17
Roadmaps: `g07.003`, `g07.004`, `g07.005`, `g07.010`
Strict lane: [`085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

## What Changed

- added the first working `effigy-codegraph` runtime:
  - ignore-aware repo walking
  - local SQLite graph storage under `.effigy/graph/graph.db`
  - file scan state and extractor version freshness tracking
- added first-party extractor framework:
  - `LanguageIndexer`
  - `GraphSink`
  - validated symbol/edge/reference/diagnostic output
- added first working extractors:
  - Rust
  - Effigy/TOML manifest first pass
  - Markdown docs first pass
  - PHP first pass
  - JavaScript / TypeScript first pass
- wired CLI and runner surface:
  - `effigy graph index`
  - `effigy graph status`
  - `effigy graph search`
  - `effigy graph files`
  - `effigy graph node`
  - `effigy graph callers`
  - `effigy graph callees`
  - `effigy graph impact`
  - `effigy graph context`
- added graph help, parse coverage, and runner JSON/text coverage
- fixed two correctness bugs found by live smoke:
  - graph subcommands with a positional argument now still parse trailing flags
  - Rust extractor IDs no longer fail on multiline impl targets or whitespace-heavy syntax
- added freshness propagation on query payloads and text-mode stale warnings

## Validation

- `cargo test -p effigy-codegraph`
- `cargo check -p effigy-cli -p effigy`
- `cargo test graph -- --nocapture`
- `./target/debug/effigy graph index --repo crates/effigy-artifacts --json`
  - `indexed_files=9`
  - `failed_paths=0`
  - `diagnostics=0`
- `./target/debug/effigy graph index --repo crates/effigy-codegraph --json`
  - `indexed_files=21`
  - `failed_paths=0`
  - `diagnostics=0`
- `./target/debug/effigy graph index --repo crates/effigy-builtin --json`
  - `indexed_files=127`
  - `failed_paths=0`
  - `diagnostics=0`
- full repo cold index:
  - command: `./target/debug/effigy graph index --repo .`
  - duration: `136.80s`
  - `indexed_files=3186`
  - `symbols=28717`
  - `edges=128716`
  - `failed=0`
- full repo follow-up timings:
  - `graph status --repo .` -> `3.27s`
  - `graph search --repo . release --limit 5` -> `5.02s`
  - `graph context --repo . "trace release orchestrator" --language rust --max-files 6` -> `2.79s`
  - direct `rg -n "release" src crates docs skills | head -n 20` -> `0.00s`
- graph DB size after full repo index:
  - `.effigy/graph/graph.db` -> `118M`

## Current State

- `903`, `904`, `905`, and `910` are ready to treat as complete.
- the graph core is usable against real Effigy crates and the full repo.
- query results now carry freshness state instead of hiding staleness.
- first-pass extractors for manifests, docs, PHP, and JS/TS are landed, but
  the deeper roadmap promises for those lanes are not fully proven yet.

## Remaining Open

- `906`
  - manifest indexer still needs composed-manifest relations, task-step
    ownership, and richer runtime-owner links
- `907`
  - docs indexer still needs code-fence metadata and stronger local path
    reference capture
- `908` / `909`
  - extractor depth and fixture proof need another pass before closeout
- `911`
  - context packs still need explicit selection reasons, snippet budgeting, and
    overflow accounting
- `912`
  - no-op index cost is still too high because `graph index` rebuilds the whole
    graph every time
  - performance proof needs a clearer statement about where graph beats direct
    `rg` and where it does not

## Vision Target Delta

- primary vision tags touched: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`
- moved:
  - no executable graph surface -> working graph index/status/query CLI
  - no extractor boundary -> first-party extractor framework
  - fixture-only storage contract -> real crate and full-repo proof
- remains open:
  - deeper manifest/docs/language extractor proof
  - context-pack richness
  - no-op index performance
