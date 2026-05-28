# 415 - Plan Artifact Contract And Example App Boundary

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-06
Completed: 2026-05-06

## Goal

Promote the first artifact substrate contract and pin the Example App boundary
before implementation starts.

## Scope

- add `014-artifact-substrate-contract.md`
- add `g03.036`
- define artifact terminology, metadata, source rules, staging rules, UAT
  apply/capture records, and security defaults
- audit Example App/Farmyard seed-bundle flow
- choose the first replacement boundary

## Boundary Decision

Effigy owns artifact transport and staging:

- local artifact classification
- explicit `oci://` ref parsing
- pull/push planning
- digest capture
- metadata synthesis
- stable staging roots
- seed/dump input normalization
- apply/capture operation reports

Farmyard owns migration semantics:

- seed-bundle family ordering
- `bundle-set.json`
- post-SQL hook indexes
- owner-scoped media request/finalization logic
- patch overlays
- residual queues and closeout gates
- database idempotency

## Example App Audit

The current Farmyard flow already has good seams:

- `seed-bundle-build.sh` packages generated `migration/dist/seed-bundles/*`
  directories into `.oci` files.
- `seed-bundle-publish.sh` publishes those `.oci` files to the local OCI store.
- `seed-bundle-install.sh` consumes either an `oci_ref` or local bundle file,
  pulls it into `migration/dist/seed-bundles/<name>`, then regenerates local
  app-owned hook artifacts.
- `seed-bundles.sources.sample.json` already models digest-pinned registry
  refs.
- `migration/dist/seed-bundles/bundle-set.json` is the app-owned replay
  manifest.
- `.underlay-local-oci` is the local development OCI store.

## First Replacement

Move only the transport/staging half of `seed-bundle-install.sh` behind Effigy
artifact staging first.

Do not replace Farmyard's replay model in this lane. Effigy should hand the app
a resolved staged artifact path/context and record what happened.

## Exit Condition

This card is complete when the contract, roadmap, and strict-lane surfaces make
the Effigy/Farmyard responsibility split clear enough to scaffold
`crates/effigy-artifacts`.

## Next Task

Card [`416-scaffold-effigy-artifacts-crate.md`](./416-scaffold-effigy-artifacts-crate.md).
