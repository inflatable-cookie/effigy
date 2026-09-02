# 1106 - Cut Over The Generated Catalog Baseline

Roadmap: [`../048-catalog-pack-publication-and-cutover.md`](../048-catalog-pack-publication-and-cutover.md)
Spec: [`../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md`](../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Ready
Owner: Effigy compiled catalog snapshot and provenance verification
Created: 2026-09-01

## Publication Input

- source repository: `inflatable-cookie/effigy-catalog-pack`
- source tag: annotated `v1.0.1`, object
  `2bb561109dfe8ec1346779370e2e9f428ef5ddd2`, peeling to
  `5ef0ec2b64612c7803cc6105a65ea462862a0b21`
- OCI manifest:
  `ghcr.io/inflatable-cookie/effigy-catalog-pack@sha256:91de584e77487765c24f53abb63413783a99c0a7926c25aee1289a3cf370d9f3`
- unpacked content identity:
  `sha256:9498d33f1eccbb91e971b55f5169830baca26326a8f802408a0432e733254974`
- accepted publication evidence: catalog-pack PR `#4`, merged as
  `7427421a3bebf207ce9979c47f60609d1b276713`

## Purpose

Replace Effigy's editable concrete catalog authority with an exact generated
recovery snapshot from the verified official artifact.

## Acceptance

- checked-in snapshot is an exact `pack/` copy and clearly generated
- typed lock records source repository/commit, pack version, OCI digest, and
  unpacked content identity
- offline QA rejects byte, manifest, version, content-identity, and lock drift
- online provenance proof pulls by digest, verifies attestation, and compares
  exact inventory and bytes
- existing compiled-baseline, bootstrap, layering, and representative catalog
  behavior stays unchanged and offline
- no public update command or Effigy release is added

## Review Oracle

Reject hand-editable snapshot authority, incomplete lock identity, network in
ordinary QA/use, content/OCI identity conflation, asset drift, or behavior that
requires an installed pack.

## Stop Conditions

Stop if the public artifact cannot reproduce exact bytes, offline proof needs
network access, or the embed path cannot remain a permanent recovery floor.

## Next Task

Implement this card. After merge, refresh readiness for cards `1107` and
`1108`; they may become a parallel frontier.
