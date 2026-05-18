# Large Repo Scale And Storage Hardening

Date: 2026-05-18  
Roadmap: [`g07.043`](../../roadmaps/g07/043-large-repo-scale-and-storage-hardening.md)  
Batch card: [`992`](../../roadmaps/g07/batch-cards/992-harden-large-repo-scale-and-storage.md)  
Strict lane: [`091`](../../specs/091-codegraph-parity-strict-lane.md)

## What Changed

- bumped graph storage schema to `2`
- added explicit storage migration handling on graph open
- backfill-migrated older graph DBs that had search rows but no indexed
  file-body `source` rows
- reject newer unknown storage schemas instead of opening them optimistically
- set a steadier local SQLite posture for graph access:
  - `journal_mode = WAL`
  - `synchronous = NORMAL`
  - `temp_store = MEMORY`
  - `busy_timeout = 5s`

## Migration Proof

New regression coverage:

- `graph_store_migrates_v1_search_index_to_source_backfill`
- `graph_store_rejects_newer_storage_schema`
- `graph_store_initializes_graph_dir_and_reopens`

What this proves:

- an older local graph DB from before source-body FTS evidence is upgraded on
  open without forcing a manual delete first
- a graph DB from a newer unknown schema fails loudly instead of risking silent
  partial reads
- the local SQLite file reopens with the expected WAL-backed posture

## Synthetic Scale Benchmark

Synthetic corpus shape:

- 450 Rust modules chained by local imports/calls
- 450 Markdown docs
- 1 Effigy manifest
- total indexed files: `902`

Measured on a warm local workstation with the built `effigy` binary:

- full `graph index --json`: `5.39s`
- incremental `graph index --json` after one touched Rust file: `1.31s`
- warm `graph status --json`: `0.11s`
- warm `graph explore "trace helper_450 ownership"`: `0.19s`
- graph DB size: `2,768,896` bytes

Observed result shape:

- index counts:
  - `files = 902`
  - `symbols = 1804`
  - `edges = 2259`
  - `references = 449`
  - `diagnostics = 0`
- incremental reindex narrowed `changed_paths` to exactly `src/module_450.rs`
- warm `status` returned no `stale_paths` and no `failed_paths`
- warm `explore` stayed inside the configured byte budget and returned:
  - `primary_count = 6`
  - `excerpt_count = 11`
  - `relation_count = 28`

## Interpretation

- the storage layer now has an explicit forward-only contract instead of
  assuming every local graph DB matches the current binary
- source-body ranking evidence survives upgrade more predictably because older
  caches are backfilled on open
- warm query costs on a synthetic repo substantially larger than Effigy remain
  comfortably below a broad fresh file-system sweep for the same question

## Residual Limits

- the benchmark is synthetic, not a corpus of real third-party repos
- this slice did not add a configurable max-index file-size policy yet
- output overflow remains controlled by the existing query byte budgets rather
  than a new suite-specific limiter

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `CONTRACT`, `MAINT`
- moved: graph storage now has versioned migration handling and measured warm
  query posture on a larger synthetic corpus
- remains open: agent workflow polish and final parity closeout

## Next Task

Execute `993`.
