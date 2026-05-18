# g07.043 - Large-Repo Scale And Storage Hardening

Status: Complete
Depends on: `g07.042`

## Goal

Keep graph indexing and querying predictable on repos much larger than Effigy.

## Scope

- benchmark against large public or synthetic fixtures without vendoring them
- measure:
  - full index time
  - incremental update time
  - `status` time
  - `explore` warm query time
  - DB size
  - output byte sizes
- add schema migration tests for every storage change in this suite
- ensure writer/reader behavior is clean during watch refresh
- evaluate SQLite settings that improve local read performance without
  sacrificing portability
- add guardrails for max file size, generated files, and ignored paths
- document how to rebuild a corrupt or incompatible graph safely

## Guardrails

- no checked-in huge fixtures
- no platform-specific DB mode without fallback
- no silent data loss during schema migration
- no benchmark pass that ignores stale or failed paths

## Acceptance Criteria

- benchmark log records scale posture before closeout
- storage migrations are tested
- large-output overflow behavior is deterministic
- common queries remain faster than broad filesystem exploration on warm index

## Evidence

- [`2026-05/18-172629-large-repo-scale-and-storage-hardening.md`](../../logs/2026-05/18-172629-large-repo-scale-and-storage-hardening.md)

## Next Task

Execute `993` after scale constraints are known.
