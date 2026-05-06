# 425 - Implement Local Artifact Capture With Planned OCI Push

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-06

## Goal

Add the first write-side artifact command without mutating a registry by
default.

## Scope

- add `effigy artifact capture <SOURCE_PATH> --ref oci://<REF>`
- stage the local source through the existing artifact metadata model
- record planned OCI destination metadata without pushing
- reject digest-pinned push destinations for capture
- support optional `--kind <KIND>` and `--environment <LABEL>` if they stay
  small
- add JSON/text reports and focused tests
- keep live push behind a later explicit card

## Non-Goals

- no live OCI push
- no container dump integration
- no credential manager
- no app migration semantics
- no `.github/workflows/` edits
- no release work

## Exit Condition

This card is complete when local capture produces staged artifact metadata plus
a planned OCI destination report, refuses invalid push refs, and gives Farmyard
enough output to consume as a local snapshot handoff.

## Closeout

- added `effigy artifact capture <SOURCE_PATH> --ref oci://<REF>`
- capture stages local payloads through `effigy.artifact.v1`
- capture reports planned OCI destination metadata with `planned=true`,
  `pushed=false`, and no registry mutation
- `--kind` and `--environment` can stamp captured metadata
- `--farmyard-handoff` emits the same handoff shape as stage
- digest-pinned destination refs are rejected
- `--push` is rejected until live push lands

## Next Task

Card
[`426-wire-planned-artifact-capture-into-container-data-dump.md`](./426-wire-planned-artifact-capture-into-container-data-dump.md).
