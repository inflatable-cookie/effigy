# 424 - Plan OCI Capture Push For UAT Snapshots

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-06

## Goal

Define the smallest safe capture/push boundary for UAT content snapshots before
adding write-side OCI behavior.

## Scope

- decide command shape for artifact capture and dump-to-OCI destinations
- define what metadata must be packaged with captured SQL/content payloads
- define digest, tag, overwrite, and immutability rules
- decide how UAT operators authenticate and authorize pushes
- decide whether `container data dump <target>=oci://...` writes directly or
  produces a local staged artifact first
- keep Acowtancy/Farmyard migration semantics app-owned

## Non-Goals

- no live push implementation in the planning card
- no credential manager
- no production data mutation
- no `.github/workflows/` edits
- no release work

## Exit Condition

This card is complete when capture/push behavior is specified tightly enough to
implement without guessing about UAT safety, tag mutability, or app ownership.

## Closeout

- `014-artifact-substrate-contract.md` now defines the write-side command
  shape:
  - `effigy artifact capture <SOURCE_PATH> --ref oci://<REF> [--kind <KIND>] [--environment <LABEL>] [--push]`
  - `effigy container data dump <TARGET>=oci://<REF> [--environment <LABEL>] [--push]`
- capture is two-phase by default: stage locally first, then push only when
  explicitly requested
- dump-to-OCI follows the same model and must not mutate a registry without
  explicit push intent
- digest-pinned refs are invalid push destinations
- pushed tags must report the immutable digest
- overwriting tags is out of the first implementation unless an explicit
  `--overwrite` flag is added later
- UAT auth uses registry-client auth such as `oras login`; tokens do not belong
  in refs or seed/dump env files
- Farmyard keeps app-level snapshot validity and layering state; Effigy owns
  outer artifact capture/stage/push reports

## Next Task

Card
[`425-implement-local-artifact-capture-with-planned-oci-push.md`](./425-implement-local-artifact-capture-with-planned-oci-push.md).
