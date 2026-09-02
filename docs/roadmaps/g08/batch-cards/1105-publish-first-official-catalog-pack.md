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
- package linkage and public visibility are confirmed explicitly; first-package
  visibility uses the documented operator package-settings control between the
  protected publish and finalize jobs, never an undocumented REST mutation
- `stable` moves only after every proof and resolves to the same digest
- previous verified channel target is recorded; when it exists, live retag
  rollback is exercised, while an absent first-publication target is proved in
  the non-mutating model and never emulated by deleting the candidate manifest
- partial same-digest retry succeeds; different-digest collision stops

## Ordered Execution

1. Land a reviewable implementation PR that turns the protected rehearsal into
   a serialized two-job publication transaction and proves every pre-mutation
   gate. No source tag, package, attestation, visibility, provider allowlist, or
   channel mutation occurs before exact-head review and merge.
2. After the orchestrator merges that reviewed head to pack `main`, continue
   with the same worker identity. Add only the exact pinned `actions/attest`
   action to this repository's selected-actions policy, create the annotated
   `v1.0.0` tag at the reviewed merged source, and dispatch the protected
   version-publish job. Stop on any gate failure with `stable` unchanged.
3. After the private first version exists, pause before finalization. The
   operator changes the linked organization package to public through GitHub's
   documented package-settings control. The protected finalize job starts only
   after that checkpoint, verifies public linkage, uses the pinned attestation
   action, pulls anonymously, refreshes Effigy support/release authority, then
   moves `stable` once. If finalization is approved prematurely, it fails
   closed and remains safely retryable.
4. Record immutable publication and rollback evidence in a follow-up PR. Card
   `1105` is not complete until that evidence is reviewed and merged.

## Review Oracle

Reject any moved/deleted source tag continuation, stale support input, overwrite
of a different OCI digest, premature channel move, authenticated-only proof,
unattested subject, missing rollback target, unrecorded mutation, an
undocumented visibility API, manifest deletion used to restore an absent
channel, support proof still pinned to the one-time import commit, or live
mutation before the implementation PR is accepted and merged.

## Stop Conditions

The 2026-09-02 instruction authorizes only the named first-publication
mutations. Stop on attestation-shape failure, anonymous-pull mismatch,
non-deterministic retry, support-input drift, tag collision, permission drift,
or any need to release Effigy or widen package/repository authority.

## Next Task

Execute the implementation-only first phase from the committed worker handoff.
Accepted publication evidence unblocks card `1106`; it does not authorize an
Effigy binary release.
