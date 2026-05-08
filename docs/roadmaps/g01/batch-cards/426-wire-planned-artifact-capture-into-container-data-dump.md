# 426 - Wire Planned Artifact Capture Into Container Data Dump

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-06

## Goal

Let `container data dump` target an OCI artifact ref without pushing to a
registry by default.

## Scope

- accept `container data dump <TARGET>=oci://<REF>`
- write the SQL dump to a local temporary/staged source
- pass that source through artifact capture
- report the planned OCI destination
- keep existing local file dump behavior unchanged
- reject live push flags unless live push is implemented in a later card
- add focused tests with no real container or registry mutation where possible

## Non-Goals

- no live OCI push
- no credential manager
- no production data mutation
- no app migration semantics
- no `.github/workflows/` edits
- no release work

## Exit Condition

This card is complete when dump-to-OCI produces the same planned capture report
as `effigy artifact capture`, local dump behavior still passes, and the command
does not mutate any registry.

## Closeout

- `container data dump <TARGET>=oci://<REF>` now preserves the OCI destination
  through output path resolution
- dump-to-OCI writes the SQL dump to `.effigy/local/data-dumps/<target>.sql`
- the local dump file is passed through the same planned capture report path as
  `effigy artifact capture`
- JSON dump reports include both `local_path` and `artifact_capture`
- existing local file dump behavior remains unchanged
- no registry push occurs

## Next Task

Card
[`427-implement-live-oci-push-through-artifact-adapter.md`](./427-implement-live-oci-push-through-artifact-adapter.md).
