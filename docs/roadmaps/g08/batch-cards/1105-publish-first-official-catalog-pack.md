# 1105 - Publish The First Official Catalog Pack

Roadmap: [`../048-catalog-pack-publication-and-cutover.md`](../048-catalog-pack-publication-and-cutover.md)
Spec: [`../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md`](../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Blocked on card `1104` and explicit operator mutation authority
Owner: protected pack publication and operator-controlled package visibility
Created: 2026-09-01

## Purpose

Create and prove the first public `v1.0.0` artifact and `stable` channel at one
immutable digest.

## Acceptance

- annotated source `v1.0.0` is protected and rechecked by object and peeled commit
- support input is fresh, internally valid, release-backed, and compatible
- OCI `v1.0.0` is created only at the deterministic candidate digest
- digest-bound attestation verifies; anonymous digest pull reproduces exact bytes
- package linkage and public visibility are confirmed explicitly
- `stable` moves only after every proof and resolves to the same digest
- previous verified channel target is recorded and rollback is exercised
- partial same-digest retry succeeds; different-digest collision stops

## Review Oracle

Reject any moved/deleted source tag continuation, stale support input, overwrite
of a different OCI digest, premature channel move, authenticated-only proof,
unattested subject, missing rollback target, or unrecorded mutation.

## Stop Conditions

No action begins without a fresh explicit operator instruction naming first
publication. Stop on attestation-shape failure, anonymous-pull mismatch,
non-deterministic retry, support-input drift, tag collision, or permission drift.

## Next Task

Blocked. Accepted publication evidence unblocks card `1106`; it does not
authorize an Effigy binary release.
