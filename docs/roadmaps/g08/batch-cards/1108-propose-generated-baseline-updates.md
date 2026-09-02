# 1108 - Propose Generated Baseline Updates

Roadmap: [`../048-catalog-pack-publication-and-cutover.md`](../048-catalog-pack-publication-and-cutover.md)
Spec: [`../../../specs/archive/115-catalog-pack-publication-and-cutover-strict-lane.md`](../../../specs/archive/115-catalog-pack-publication-and-cutover-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Complete
Owner: pack-repository GitHub App baseline proposal path
Created: 2026-09-01
Promoted: 2026-09-02 — card `1106` merged at `6271b0ff129d006e47202b1b00def5ea7a395af8`
Closed: 2026-09-02 — proposal automation merged in catalog-pack PR 5 at
`4dd8b8a5`; the narrowly installed App and empty-delta provider checkpoint are
recorded in catalog-pack PR 6, merged at `ebb813e1`

## Purpose

Let a verified pack publication propose, but never accept or release, an exact
generated Effigy baseline update.

## Acceptance

- short-lived GitHub App token is narrowed to Effigy contents and pull requests
- proposal changes only the generated snapshot, lock, and required evidence
- the job cannot approve, merge, alter Effigy workflows/product code, or release
- Effigy independently reruns offline drift and public-artifact provenance proof
- pack publication success does not depend on Effigy accepting the proposal

## Validation

- network-free workflow/model tests prove exact generated-only path allowlist,
  immutable artifact input, short-lived token request, and no approve/merge/
  release path
- adversarial diffs reject Effigy product code, workflows, unrelated docs,
  hand-edited snapshot bytes, and incomplete lock/evidence changes
- Effigy-side verification reruns the committed offline baseline proof and
  public digest/attestation/exact-byte proof independently of pack publication
- hosted execution is a separate provider checkpoint: do not register/install
  an App, write secrets, dispatch, or mutate Effigy without explicit operator
  authorization after the implementation PR is accepted
- pack repository doctor/validate/QA and workflow guards pass; proposed Effigy
  branch validation uses current pushed Effigy `main`

## Evidence

Catalog-pack PR 5 records the exact App permission request, repository
allowlist, pinned action identities, generated-path policy, no-mutation proof,
and every oracle counterexample. PR 6 records the operator-authorized provider
checkpoint: the App is narrowly installed on Effigy, the published digest and
Effigy snapshot/lock are already exact, and no known-no-op proposal was
dispatched. The first non-empty live proposal remains operational evidence for
a future published digest, not unfinished implementation or release authority.

## Review Oracle

Reject PAT use, broad repository/token scope, unrelated edits, self-approval or
merge, release authority, unverified artifact input, or publication dependence
on proposal acceptance.

## Stop Conditions

Stop if the App cannot be narrowly installed, generated-only scope cannot be
enforced, or Effigy cannot independently reproduce the proposal.

## Next Task

Return to the operator intent checkpoint. Do not manufacture a pack release or
proposal delta to exercise the live path.
