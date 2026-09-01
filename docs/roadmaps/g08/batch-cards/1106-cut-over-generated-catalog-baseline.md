# 1106 - Cut Over The Generated Catalog Baseline

Roadmap: [`../048-catalog-pack-publication-and-cutover.md`](../048-catalog-pack-publication-and-cutover.md)
Spec: [`../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md`](../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Blocked on accepted card `1105` evidence
Owner: Effigy compiled catalog snapshot and provenance verification
Created: 2026-09-01

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

Blocked. After merge, cards `1107` and `1108` may become a parallel frontier.
