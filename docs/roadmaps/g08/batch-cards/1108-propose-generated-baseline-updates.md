# 1108 - Propose Generated Baseline Updates

Roadmap: [`../048-catalog-pack-publication-and-cutover.md`](../048-catalog-pack-publication-and-cutover.md)
Spec: [`../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md`](../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Ready
Owner: pack-repository GitHub App baseline proposal path
Created: 2026-09-01
Promoted: 2026-09-02 — card `1106` merged at `6271b0ff129d006e47202b1b00def5ea7a395af8`

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

The implementation PR records the exact App permission request, repository
allowlist, pinned action identities, generated-path policy, no-mutation proof,
and every oracle counterexample. Live App installation/token/PR evidence is a
second phase only after explicit operator authorization; absence of that gate
must leave publication independent and the card honestly paused, not weakened.

## Review Oracle

Reject PAT use, broad repository/token scope, unrelated edits, self-approval or
merge, release authority, unverified artifact input, or publication dependence
on proposal acceptance.

## Stop Conditions

Stop if the App cannot be narrowly installed, generated-only scope cannot be
enforced, or Effigy cannot independently reproduce the proposal.

## Next Task

Implement the no-provider-mutation phase from current pushed catalog-pack
`main`. It may run in parallel with card `1107`; this lane owns only the pack
repository workflow/scripts/tests/docs and its evidence. Effigy planning,
product code, workflows, release state, and shared front-door closeout remain
orchestrator-owned. Stop after the implementation PR unless the operator
explicitly authorizes the live GitHub App/provider phase.
