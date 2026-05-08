# 417 - Local Artifact Staging For Seed Inputs

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-06
Completed: 2026-05-06

## Goal

Implement local artifact staging for seed inputs so local `.sql`, `.sql.gz`,
and `.dump` files resolve through the same artifact metadata model that OCI
sources will use later.

## Scope

- add local staging helpers to `effigy-artifacts`
- copy local payloads into a controlled `.effigy/local/artifacts/...` shape
- write synthesized `effigy-artifact.json` metadata
- return `StagedArtifactReport`
- add tests for deterministic staging roots, metadata paths, and primary
  payload paths
- keep behavior independent from bootstrap/container integration for this card

## Non-Goals

- no OCI pull/push
- no public `effigy artifact` command
- no bootstrap or container data wiring yet
- no Acowtancy file edits

## Exit Condition

This card is complete when local SQL-like artifact sources can be staged by
`effigy-artifacts` and `cargo test -p effigy-artifacts` passes.

## Closeout

Added local staging helpers in `effigy-artifacts`.

The crate now can:

- resolve relative local artifact paths against a base directory
- stage payloads under `.effigy/local/artifacts`
- write `effigy-artifact.json`
- return `StagedArtifactReport`
- reject missing local source files cleanly

Validation passed:

```sh
cargo test -p effigy-artifacts
```

## Next Task

Card [`418-oci-pull-inspect-and-stage.md`](./418-oci-pull-inspect-and-stage.md).
