# 687 - Split Artifact Internals Or Document Deferral

Roadmap: [`../039-artifact-and-crate-boundary-rejustification.md`](../039-artifact-and-crate-boundary-rejustification.md)
Strict lane: [`../../../specs/075-artifact-and-crate-boundary-review-strict-lane.md`](../../../specs/075-artifact-and-crate-boundary-review-strict-lane.md)
Contract: [`../../../contracts/031-artifact-and-crate-boundary-contract.md`](../../../contracts/031-artifact-and-crate-boundary-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Split `effigy-artifacts` internals by stable concern if the ownership map
supports it.

## Acceptance

- `lib.rs` becomes a facade over bounded modules or deferral is documented
- artifact tests pass
- public imports remain compatible

## Outcome

- split `crates/effigy-artifacts/src/lib.rs` into bounded modules:
  `refs`, `metadata`, `staging`, `oci`, `reports`, `errors`, and `util`
- kept `lib.rs` as the compatibility facade and test owner
- reduced `lib.rs` from 1,334 lines to a focused facade/test file

## Validation

- `cargo test -p effigy-artifacts`
- `cargo check --bin effigy`
- `cargo fmt --all -- --check`

## Next Task

Execute `688` to review small crate ownership.
