# Graph Follow-Up Baseline Inventory

Date: 2026-05-18  
Roadmap: [`g07.013`](../roadmaps/g07/013-graph-follow-up-performance-and-fixture-reliability.md)  
Batch card: [`931`](../roadmaps/g07/batch-cards/931-baseline-incremental-query-and-failed-path-inventory.md)  
Strict lane: [`086`](../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md)

## What Changed

- locked the follow-up baseline against the `g07.012` closeout evidence
- classified the seven known failed full-repo graph paths into reusable
  failure buckets
- identified the first profitable incremental-index seam
- identified the first profitable query hot path

## Locked Baseline

From the `g07.012` closeout:

- cold full-repo index: `144.14s`
- no-op full-repo index: `148.39s`
- `graph status --json`: `2.34s`
- `graph search release --limit 5 --json`: `4.43s`
- `graph context "trace release orchestrator" --language rust --max-files 6 --max-bytes 2048 --json`: `2.73s`
- full-repo failed paths: `7`

These are the comparison numbers for `932`, `933`, and `935`.

## Failed-Path Inventory

### Failure Class A: Template-Rich Bundle Export Surfaces Parsed As Plain TOML

These files contain Jinja-style template control flow or bundle helper calls
that the current manifest/TOML indexer attempts to parse as raw TOML:

- `crates/effigy-manifest/tests/fixtures/php-app-bundle/export.toml`
- `crates/effigy-manifest/tests/fixtures/php-library-bundle/export.toml`
- `crates/effigy-manifest/tests/fixtures/workspace-app-bundle/export.toml`
- `external/bundles/underlay/export.toml`

This is the first target for `934`.

### Failure Class B: Bundle Manifests With Template Interpolation Or Template-Backed Composition

These manifests either embed template syntax directly or depend on bundle
composition that currently falls into the same unsupported path:

- `external/bundles/decodelabs-library/effigy.toml`
- `external/bundles/decodelabs/effigy.toml`
- `examples/render-provider-smoke/effigy.toml`

`examples/render-provider-smoke/effigy.toml` is a consumer manifest, but its
bundle base points at template-heavy workspace-app bundle fixtures, so the
semantic graph path inherits the same parsing gap.

This is the second target for `934`.

## First Profitable Incremental-Index Seam

The current index path in
[index.rs](../../../../crates/effigy-codegraph/src/index.rs)
still:

- scans the repo every run
- clears all graph data every run
- rereads every file every run
- reruns every extractor every run
- rebuilds the search index every run

The first profitable seam is file-level reuse keyed by:

- relative path
- content hash
- language id
- extractor version

That supports a no-op short path and a changed-slice path without changing the
public graph contract.

This is the concrete target for `932`.

## First Profitable Query Hot Path

The current query layer in
[query/mod.rs](../../../../crates/effigy-codegraph/src/query/mod.rs)
still pays high projection cost by loading broad record sets into memory for
common commands:

- `status` recomputes freshness by rescanning the repo and hashing files
- `search` depends on the FTS table, but still resolves follow-up record detail
  one record at a time
- `context` loads whole file/symbol/edge collections before ranking and snippet
  selection

The first profitable speed target is to reduce broad whole-store materialization
for `status`, `search`, and `context` before attempting more exotic caching.

This is the concrete target for `933`.

## Next Concrete Targets

- `932`: file-level incremental index reuse and no-op short path
- `933`: thinner query projections and lower context assembly cost
- `934`: template-aware or deliberately classified fixture/bundle manifest
  indexing

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved: qualitative follow-up intent -> fixed baseline, explicit failed-path
  classes, and concrete optimization seams
- remains open: `932`, `933`, `934`, `935`
