# 423 - Wire OCI Artifact Refs Into Seed And Dump Surfaces

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-06

## Goal

Let the existing seed/dump flows consume and produce artifact refs without
making callers care whether the source is a local SQL file or an OCI artifact.

## Scope

- allow `bootstrap --db-seed <target>=oci://...`
- allow `container data seed --db-seed <target>=oci://...`
- preserve current local SQL seed behavior and legacy staged seed handoff
- decide the first bounded behavior for `container data dump <target>=oci://...`
- keep live OCI push/capture behind a planned boundary if it is not ready
- add focused tests with fake artifact transport or local staged fixtures
- keep app migration semantics outside Effigy

## Non-Goals

- no generic migration framework
- no OCI push unless the boundary is small and already proven
- no public credential manager
- no `.github/workflows/` edits
- no release work

## Exit Condition

This card is complete when seed flows can resolve OCI artifact refs through the
same staged primary-file model as local SQL files, existing local seed tests
still pass, and dump behavior is either implemented for local artifact output or
explicitly parked behind the capture/push card.

## Closeout

- `bootstrap --db-seed <target>=oci://...` now preserves the artifact ref
  through path resolution and stages it through the shared artifact model
- `container data seed --db-seed <target>=oci://...` uses the same staging path
  because both flows share `stage_db_seed_files`
- local SQL seed behavior and the legacy `.effigy/local/db-seeds` handoff are
  unchanged
- focused fake-adapter tests cover OCI seed staging without a real registry
- `container data dump <target>=oci://...` is explicitly parked behind the
  capture/push card because push semantics need a separate durable boundary

## Next Task

Card
[`424-plan-oci-capture-push-for-uat-snapshots.md`](./424-plan-oci-capture-push-for-uat-snapshots.md).
