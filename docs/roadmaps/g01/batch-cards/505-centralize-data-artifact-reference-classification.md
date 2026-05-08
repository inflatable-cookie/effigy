# 505 - Centralize Data Artifact Reference Classification

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Remove runner-local `oci://` path classification from DB seed and dump paths.

## Scope

- expose a small `effigy-data` helper for `oci://` data artifact references
- migrate `resolve_db_seed_input_paths` to use the helper
- migrate `resolve_db_dump_output_paths` to use the helper
- remove duplicate runner-local `starts_with("oci://")` helpers
- preserve path expansion behavior exactly

## Non-Goals

- no artifact staging migration yet
- no logical target resolution migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when runner DB seed/dump path expansion no longer owns
the raw OCI reference test.

## Closeout

Exposed `is_oci_artifact_ref_path` from `effigy-data` and migrated DB seed and
dump path expansion to use it. The raw `oci://` test is now centralized in the
data crate.

## Validation

- `cargo test -p effigy-data` passed
- `cargo test -p effigy --lib container_command::data` passed
- `cargo test -p effigy --lib db_seed` passed
- `git diff --check` passed

## Next Task

Start card
[`506-move-logical-data-target-model-into-effigy-data.md`](./506-move-logical-data-target-model-into-effigy-data.md).
