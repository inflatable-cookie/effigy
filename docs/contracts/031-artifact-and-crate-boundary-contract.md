# Artifact And Crate Boundary Contract

Generation: `g04`
Roadmap: [`../roadmaps/g04/039-artifact-and-crate-boundary-rejustification.md`](../roadmaps/g04/039-artifact-and-crate-boundary-rejustification.md)
Strict lane: [`../specs/075-artifact-and-crate-boundary-review-strict-lane.md`](../specs/075-artifact-and-crate-boundary-review-strict-lane.md)
Status: Accepted
Owner: Platform
Updated: 2026-05-12

## Purpose

Define the safe boundary for reviewing artifact internals and current crate
ownership after the v0.6.x release.

## Hard Boundaries

- no public artifact API removals
- no OCI protocol redesign
- no media/object-store implementation
- no automatic crate creation
- no automatic crate merging
- no release execution
- no `.github/workflows/` edits

## Artifact Ownership

`effigy-artifacts` owns reusable artifact substrate concerns:

- artifact source refs
- artifact metadata and operation reports
- local staging
- OCI request/report models
- ORAS-backed OCI adapter
- artifact/OCI error types

## Accepted Artifact Module Split

The accepted module split is:

- `refs`: local/OCI source refs, source types, artifact kinds, and ref parsing
- `metadata`: artifact metadata schema and builder
- `staging`: local and pulled-OCI staging requests, reports, copy logic, and
  metadata writes
- `oci`: OCI request/report models, ORAS adapter, descriptor parsing, and
  ORAS failure remediation
- `reports`: operation report model and operation/result enums
- `errors`: ref, staging, and OCI error families
- `util`: private path, slug, digest, and redaction helpers

`lib.rs` remains a public compatibility facade.

Runner code owns command orchestration and side effects around those APIs.
`effigy-data` owns database target and seed/dump planning, not artifact
transport.

## Crate Boundary Rule

Small crates are retained when they own a stable domain boundary used outside
their original extraction site. A crate is a merge candidate only when its API is
too weak to justify a dependency boundary and callers prove the boundary adds no
clarity.

## Small Crate Review Outcome

The current small crates are retained by ownership, not size:

- `effigy-core`: bottom utility layer
- `effigy-runtime-plan`: pure activation planning/report model
- `effigy-process`: host process primitives
- `effigy-routing`: selector routing and task lookup order
- `effigy-gateway`: local gateway registry and route primitives
- `effigy-ui`: renderer abstraction and output primitives

No immediate crate merge candidate is accepted in this lane.

## Accepted Outcome

`effigy-artifacts` no longer has a god-file implementation shape. Its public
API remains facade-backed and focused tests pass.

The final god-file scan for this lane reported only two warning-level files:

- `src/runner/state_command.rs`
- `crates/effigy-release/src/lib.rs`

Those are future cleanup candidates, not blockers for this lane.

## Acceptance Boundary

This contract is satisfied when:

- `effigy-artifacts` internals are split or explicitly deferred with evidence
- current small-crate ownership is documented
- package-map docs describe the accepted artifact and crate-boundary posture
- public behavior remains stable under focused tests
