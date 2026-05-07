# 507 - Move Seed Source Normalization into Effigy Data

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move pure DB seed source normalization into `effigy-data`.

## Scope

- add a pure seed input path normalization helper to `effigy-data`
- preserve local relative path joining against cwd
- preserve absolute path passthrough
- preserve `oci://` reference passthrough
- migrate `resolve_db_seed_input_paths` to the shared helper
- keep artifact staging and OCI pull side effects in the runner

## Non-Goals

- no artifact staging migration yet
- no dump output normalization migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when DB seed path normalization is no longer implemented
inside `src/runner/db_seed.rs`.

## Closeout

Added `normalize_seed_source_path` to `effigy-data` and migrated
`resolve_db_seed_input_paths` to use it. Local relative paths, absolute paths,
and `oci://` references retain the previous behavior.

## Validation

- `cargo test -p effigy-data` passed
- `cargo test -p effigy --lib db_seed` passed
- `git diff --check` passed

## Next Task

Start card
[`508-move-dump-destination-normalization-into-effigy-data.md`](./508-move-dump-destination-normalization-into-effigy-data.md).
