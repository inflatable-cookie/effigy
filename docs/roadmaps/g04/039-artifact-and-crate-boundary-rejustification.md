# 039 - Artifact And Crate Boundary Rejustification

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-12
Depends on:
- [`038-docs-policy-cli-help-and-test-fixture-deduplication.md`](./038-docs-policy-cli-help-and-test-fixture-deduplication.md)

## Goal

Review artifact internals and crate boundaries after the v0.6.x release so the
package map reflects current ownership rather than historical extraction steps.

## Evidence

- `crates/effigy-artifacts/src/lib.rs` is 1,334 lines in one file
- several small crates may still be justified, but need current ownership notes
- `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`
  is useful but partly historical after multiple completed extraction waves
- object-store/media migration work will increase pressure on artifact and
  state boundaries

## Scope

- split `effigy-artifacts` internally by stable concern if inspection supports it
- review small crates for clear ownership, not line count
- document why retained small crates exist
- identify crates that should merge only if their ownership is weak and call
  sites prove no useful boundary remains
- refresh crate-boundary documentation or add a concise current-status note
- keep artifact public APIs stable unless a breaking cleanup is explicitly
  accepted

## Non-Goals

- no automatic crate merging
- no automatic crate creation
- no OCI protocol redesign
- no media/object-store implementation
- no release workflow changes
- no speculative abstraction for future providers or bundle sources

## Candidate Artifact Modules

Possible internal modules:

- `refs`
- `metadata`
- `staging`
- `oci`
- `reports`
- `errors`

The implementation should choose the smallest split that reduces context load.

## Crate Boundary Rules

- more crates is not automatically better
- fewer crates is not automatically better
- merge only when a crate has no durable ownership boundary
- split only when a domain API can remain stable without importing runner logic
- preserve dependency direction from shell to domain crates

## Acceptance Criteria

- artifact internals are easier to navigate or explicitly deferred with reasons
- small crates have current ownership justification or concrete merge candidates
- package-map or crate-boundary docs reflect the current architecture
- no public behavior changes are introduced accidentally
- future media/object-store roadmap work has a clear artifact boundary to depend
  on

## Outcome

- split `effigy-artifacts` into `refs`, `metadata`, `staging`, `oci`,
  `reports`, `errors`, and private `util`
- kept `lib.rs` as the public compatibility facade
- documented current small-crate retention posture in the package map
- found no immediate crate merge candidates with sufficient ownership evidence
- confirmed artifact/media/object-store follow-up should depend on
  `effigy-artifacts`, not rebuild artifact primitives in runner/app code

## Suggested Batch Cards

- `685-open-artifact-and-crate-boundary-review-lane.md`
- `686-map-artifact-internal-ownership.md`
- `687-split-artifact-internals-or-document-deferral.md`
- `688-review-small-crate-ownership-and-merge-candidates.md`
- `689-refresh-package-map-and-crate-boundary-docs.md`
- `690-close-reference-grade-cleanup-suite.md`

## Validation

- artifact crate tests
- package-map docs review
- `cargo check --all-targets`
- `effigy scan god-files --json`
- `git diff --check`

## Next Task

Decide whether to close `g04` or roll over into the next generation.
