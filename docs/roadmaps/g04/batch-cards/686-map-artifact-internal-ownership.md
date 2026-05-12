# 686 - Map Artifact Internal Ownership

Roadmap: [`../039-artifact-and-crate-boundary-rejustification.md`](../039-artifact-and-crate-boundary-rejustification.md)
Strict lane: [`../../../specs/075-artifact-and-crate-boundary-review-strict-lane.md`](../../../specs/075-artifact-and-crate-boundary-review-strict-lane.md)
Contract: [`../../../contracts/031-artifact-and-crate-boundary-contract.md`](../../../contracts/031-artifact-and-crate-boundary-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Classify `effigy-artifacts` internals before moving code.

## Scope

- map refs, metadata, staging, OCI adapter, reports, errors, and utility helpers
- decide the smallest safe module split
- keep public exports stable

## Acceptance

- artifact ownership map is recorded
- first split shape is selected
- `687` can move code without a fresh planning pass

## Outcome

Artifact internals were classified into these stable owners:

- `refs`: local/OCI source refs, source types, artifact kinds, and ref parsing
- `metadata`: artifact metadata schema and metadata builder
- `staging`: local and pulled-OCI staging requests, reports, and metadata writes
- `oci`: OCI request/report models, ORAS adapter, descriptor parsing, and ORAS
  failure remediation
- `reports`: operation report model and operation/result enums
- `errors`: ref, staging, and OCI error families
- `util`: private path, slug, digest, and redaction helpers

The first split shape selected for `687` is a facade `lib.rs` that re-exports
the same public API from those modules.

## Next Task

Execute `687` to split artifact internals.
