# 428 - Decide Container Data Dump Live Push Boundary

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-06

## Goal

Decide whether `container data dump <TARGET>=oci://<REF>` should gain live push
or remain a planned-capture surface that requires an explicit artifact command
for publication.

## Scope

- compare operator safety for one-command dump-and-push versus two-step capture
  then push
- define any required flags such as `--push` or `--overwrite`
- decide whether container dump should ever mutate registries from automation
- keep UAT snapshot workflows explicit and auditable
- update the artifact contract and next implementation card

## Non-Goals

- no implementation in this decision card
- no credential manager
- no production data mutation
- no `.github/workflows/` edits
- no release work

## Exit Condition

This card is complete when the live push boundary for container data dump is
explicit and the next implementation or stop condition is clear.

## Decision

Initial decision: keep `container data dump <TARGET>=oci://<REF>`
planned-only.

The data dump surface may create a local dump and package/report a planned OCI
artifact destination, but it must not mutate a registry in the current contract.
Live registry writes stay on the explicit artifact surface:

```sh
effigy artifact capture <LOCAL_DUMP_PATH> --ref oci://<REF> --push
```

Reasons:

- data dump is often run from automation and broad maintenance flows
- UAT snapshot publication needs a clear operator action and audit point
- staged dump inspection is useful before publishing mutable tags
- the artifact surface already owns push, digest reporting, redaction, and
  future overwrite policy

One-command dump-and-push was later implemented in card `430` with explicit
`--push`. It must not become the default behavior for `container data dump`.

## Closeout

- `014-artifact-substrate-contract.md` now states that container data dump does
  not mutate registries without explicit push intent.
- planned dump-to-OCI plus explicit `artifact capture --push` remains available
  as the most inspectable UAT-safe workflow.
- user later opted into the one-command workflow, implemented separately in
  card `430` with explicit `--push`.

## Next Task

Close the artifact substrate lane.
