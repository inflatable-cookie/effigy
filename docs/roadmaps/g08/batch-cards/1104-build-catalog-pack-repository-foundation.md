# 1104 - Build The Catalog-Pack Repository Foundation

Roadmap: [`../048-catalog-pack-publication-and-cutover.md`](../048-catalog-pack-publication-and-cutover.md)
Spec: [`../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md`](../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Complete
Owner: public `inflatable-cookie/effigy-catalog-pack` source and validation
Created: 2026-09-01

## Purpose

Create the dedicated source repository, import exact canonical assets under
`pack/`, and prove deterministic publication locally without creating public
release state.

The operator selected public source-repository visibility on 2026-09-01. That
choice authorizes repository creation for this card; it does not authorize a
source tag, package creation, package-visibility change, or publication.

## Acceptance

- the dedicated source repository exists publicly without any release or
  package state
- source `pack/` exactly matches Effigy's current concrete catalog tree plus the
  new top-level `pack.toml` at version `1.0.0`
- repository tasks validate manifest, inventory, compatibility, content identity,
  and deterministic local OCI layout
- read-only CI and protected manual publication workflow are scoped and pinned
- the workflow consumes Effigy's support file by resolved commit/blob and proves
  absent/same-digest/collision handling in a no-push rehearsal
- a current Effigy binary installs the local pack and replays representative
  service/workspace assembly
- no source tag, package, visibility, attestation, or `stable` mutation occurs

## Review Oracle

Reject a private source repository, byte drift, assets outside `pack/`,
wall-clock-dependent digest input, package-write credentials in validation,
mutable-tag overwrite, a second compatibility policy, or any live publication
during rehearsal.

## Validation And Evidence

Record exact imported inventory, content identity, deterministic candidate
digest replay, support-input commit/blob, local Effigy acquisition proof,
workflow permission review, and no-push evidence.

## Stop Conditions

Stop on card `1103` drift, nondeterministic artifact shape, inability to attest
the generic OCI subject in principle, repository-creation authority failure, or
any need to publish while proving the foundation.

## Next Task

Card `1104` merged in external PR
[`inflatable-cookie/effigy-catalog-pack#1`](https://github.com/inflatable-cookie/effigy-catalog-pack/pull/1)
at `168b9f530d51f666007663215207a4d9dcfc9c8b`. Request explicit operator
authority for card `1105`; do not create the source tag, package, attestation,
or `stable` state before that gate.
