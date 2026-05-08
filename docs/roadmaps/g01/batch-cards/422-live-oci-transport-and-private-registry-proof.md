# 422 - Live OCI Transport And Private Registry Proof

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-06

## Goal

Add the first live OCI artifact pull/inspect transport proof while preserving
private-registry safety.

## Scope

- select a small OCI client boundary for authenticated `inspect` and `pull`
- keep credentials out of logs, JSON payloads, and operation records
- support explicit `oci://` refs only
- wire live OCI inspect into `effigy artifact inspect`
- wire live OCI pull plus staging into `effigy artifact stage`
- keep local artifact staging behavior unchanged
- add focused tests with fake transport fixtures before any live registry proof
- document how Acowtancy/UAT should provide credentials without env-file seed
  configuration

## Non-Goals

- no OCI push/capture yet
- no migration semantics in Effigy
- no public credential manager
- no implicit registry refs
- no `.github/workflows/` edits
- no release work

## Exit Condition

This card is complete when `effigy artifact inspect/stage oci://...` can use a
live authenticated transport through the artifact adapter boundary, reports
redact private registry details correctly, and the fake-transport tests prove
the command layer without requiring a real registry.

## Closeout

- live OCI inspect/pull is wired through a runner-side `oras` adapter
- `effigy artifact inspect oci://...` resolves descriptor data through the
  adapter
- `effigy artifact stage oci://...` pulls into `.effigy/local/artifacts/.oci-pulls`
  and stages primary files through the shared artifact metadata model
- fake adapter tests prove inspect, pull, staging, and Farmyard handoff without
  a real registry
- OCI userinfo is redacted at the artifact display/descriptor boundary and from
  process errors
- `014-artifact-substrate-contract.md` documents UAT/private registry auth via
  `oras login` or equivalent registry-client auth, not env-file seed selection

## Next Task

Card
[`423-wire-oci-artifact-refs-into-seed-and-dump-surfaces.md`](./423-wire-oci-artifact-refs-into-seed-and-dump-surfaces.md).
