# 1105 - Publish The First Official Catalog Pack

Roadmap: [`../048-catalog-pack-publication-and-cutover.md`](../048-catalog-pack-publication-and-cutover.md)
Spec: [`../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md`](../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Ready
Owner: protected pack publication and operator-controlled package visibility
Created: 2026-09-01
Authorized: 2026-09-02 — annotated `v1.0.0` tag, public GHCR package,
digest-bound attestation, and `stable` movement

## Purpose

Create and prove the first public `v1.0.0` artifact and `stable` channel at one
immutable digest.

## Acceptance

- annotated source `v1.0.0` is protected and rechecked by object and peeled commit
- support input resolves from Effigy's current default-branch commit, records
  that commit and blob, and is fresh, internally valid, release-backed, and
  compatible; the immutable one-time import pin is not reused as current
  support authority
- OCI `v1.0.0` is created only at the deterministic candidate digest
- digest-bound attestation verifies; anonymous digest pull reproduces exact bytes
- package linkage and public visibility are confirmed explicitly
- `stable` moves only after every proof and resolves to the same digest
- previous verified channel target is recorded and rollback is exercised
- partial same-digest retry succeeds; different-digest collision stops

## Ordered Execution

1. Land a reviewable implementation PR that turns the protected rehearsal into
   the bounded publication transaction and proves every pre-mutation gate. No
   source tag, package, attestation, visibility, or channel mutation occurs
   before exact-head review and merge.
2. After the orchestrator merges that reviewed head to pack `main`, continue
   with the same worker identity. Create the annotated `v1.0.0` tag at the
   reviewed merged source, dispatch the protected workflow, and stop on any
   gate failure.
3. Record immutable publication and rollback evidence in a follow-up PR. Card
   `1105` is not complete until that evidence is reviewed and merged.

## Review Oracle

Reject any moved/deleted source tag continuation, stale support input, overwrite
of a different OCI digest, premature channel move, authenticated-only proof,
unattested subject, missing rollback target, unrecorded mutation, support proof
still pinned to the one-time import commit, or live mutation before the
implementation PR is accepted and merged.

## Stop Conditions

The 2026-09-02 instruction authorizes only the named first-publication
mutations. Stop on attestation-shape failure, anonymous-pull mismatch,
non-deterministic retry, support-input drift, tag collision, permission drift,
or any need to release Effigy or widen package/repository authority.

## Next Task

Execute the implementation-only first phase from the committed worker handoff.
Accepted publication evidence unblocks card `1106`; it does not authorize an
Effigy binary release.
