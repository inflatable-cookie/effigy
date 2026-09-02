# 1105 - Publish The First Official Catalog Pack

Roadmap: [`../048-catalog-pack-publication-and-cutover.md`](../048-catalog-pack-publication-and-cutover.md)
Spec: [`../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md`](../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Complete
Owner: protected pack publication and operator-controlled package visibility
Created: 2026-09-01
Authorized: 2026-09-02 — preserve the failed immutable `v1.0.0` source tag;
publish the repaired first public artifact as `v1.0.1`, with public GHCR
visibility, digest-bound attestation, and `stable` movement

## Purpose

Create and prove the first public `v1.0.1` artifact and `stable` channel at one
immutable digest. Retain the failed pre-push `v1.0.0` source tag as incident
evidence; never move, delete, or reuse it.

## Acceptance

- failed annotated source `v1.0.0` remains protected at
  `f70637abe1024cf7b54cabe58c3bd5877dcf8eca`; the new annotated source
  `v1.0.1` is created only from the separately reviewed repair head and is
  rechecked by object and peeled commit
- support input resolves from Effigy's current default-branch commit, records
  that commit and blob, and is fresh, internally valid, release-backed, and
  compatible; the immutable one-time import pin is not reused as current
  support authority
- OCI `v1.0.1` is created only at the deterministic candidate digest; no OCI
  `v1.0.0` package version is invented after the failed pre-push run
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

1. The implementation PR is accepted and merged at
   `f70637abe1024cf7b54cabe58c3bd5877dcf8eca`. The selected-actions policy now
   contains exactly the pinned checkout and attest actions.
2. The first protected run created annotated `v1.0.0` at that merge and failed
   before package write because live ORAS 1.3.3 reported an absent GHCR
   descriptor as `failed to find ...: not found`, a shape absent from the
   network-free oracle. Run `33622687650` failed; package, attestation, and
   `stable` remain absent.
3. Preserve `v1.0.0`. On the same worker lane, land one repair PR that adds the
   exact live stderr fixture and narrow absence classification, updates the
   provider-control oracle for the already-authorized attest pin, bumps the pack
   to `1.0.1`, and reconciles publication docs/evidence. Ordinary validation
   remains write-free. No second live attempt occurs before exact-head review
   and merge.
4. From that reviewed merge, create annotated `v1.0.1` and dispatch the same
   protected version-publish job. Stop on any gate failure with `stable`
   unchanged. Never delete, move, or recreate `v1.0.0`.
5. After the private first version exists, pause before finalization. The
   operator changes the linked organization package to public through GitHub's
   documented package-settings control. The protected finalize job starts only
   after that checkpoint, verifies public linkage, uses the pinned attestation
   action, pulls anonymously, refreshes Effigy support/release authority, then
   moves `stable` once. If finalization is approved prematurely, it fails
   closed and remains safely retryable.
6. Record immutable incident, publication, and rollback evidence in the repair
   or a follow-up evidence PR. Card
   `1105` is not complete until that evidence is reviewed and merged.

## Validation

- exact live ORAS stderr fixture passes as remote absence; credential/tool,
  auth, timeout, and generic local `not found` fixtures still fail closed
- pack manifest, source identity, candidate reference, docs, and workflow proofs
  agree on `1.0.1` / `v1.0.1`
- `effigy doctor`, `effigy validate`, `effigy qa`, deterministic candidate
  replay, and provider controls pass; ordinary validation performs no write
- live read-back proves `v1.0.0` still names tag object
  `f2b59e65b1938600907de8dea566ad957e63be69`, no package or `stable` exists,
  and selected-actions contains exactly the two authorized pinned actions
- repair/evidence records failed run `33622687650`, its first-read stop point,
  and every retained or absent provider identity

## Review Oracle

Reject any moved/deleted source tag continuation, stale support input, overwrite
of a different OCI digest, premature channel move, authenticated-only proof,
unattested subject, missing rollback target, unrecorded mutation, an
undocumented visibility API, manifest deletion used to restore an absent
channel, support proof still pinned to the one-time import commit, or live
mutation before the implementation PR is accepted and merged.

Also reject a classifier repair that lacks the exact live ORAS
`Error response from registry: failed to find "<ref>": <ref>: not found`
counterexample, treats a generic local `not found` as registry absence, executes
new scripts against the old `v1.0.0` source identity, or retries publication
before the `v1.0.1` repair head is accepted and merged.

## Stop Conditions

The 2026-09-02 recovery decision authorizes only the named `v1.0.1`
first-publication mutations and preservation of failed `v1.0.0`. Stop on
attestation-shape failure, anonymous-pull mismatch,
non-deterministic retry, support-input drift, tag collision, permission drift,
or any need to release Effigy or widen package/repository authority.

## Next Task

Proceed to ready card
[`1106`](./1106-cut-over-generated-catalog-baseline.md). Effigy binary release
authority remains separate.
