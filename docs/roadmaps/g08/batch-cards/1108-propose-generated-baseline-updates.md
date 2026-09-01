# 1108 - Propose Generated Baseline Updates

Roadmap: [`../048-catalog-pack-publication-and-cutover.md`](../048-catalog-pack-publication-and-cutover.md)
Spec: [`../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md`](../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Blocked on card `1106`
Owner: pack-repository GitHub App baseline proposal path
Created: 2026-09-01

## Purpose

Let a verified pack publication propose, but never accept or release, an exact
generated Effigy baseline update.

## Acceptance

- short-lived GitHub App token is narrowed to Effigy contents and pull requests
- proposal changes only the generated snapshot, lock, and required evidence
- the job cannot approve, merge, alter Effigy workflows/product code, or release
- Effigy independently reruns offline drift and public-artifact provenance proof
- pack publication success does not depend on Effigy accepting the proposal

## Review Oracle

Reject PAT use, broad repository/token scope, unrelated edits, self-approval or
merge, release authority, unverified artifact input, or publication dependence
on proposal acceptance.

## Stop Conditions

Stop if the App cannot be narrowly installed, generated-only scope cannot be
enforced, or Effigy cannot independently reproduce the proposal.

## Next Task

Blocked. It may run in parallel with card `1107` only after card `1106` merges.
