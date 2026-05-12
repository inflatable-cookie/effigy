# 688 - Review Small Crate Ownership And Merge Candidates

Roadmap: [`../039-artifact-and-crate-boundary-rejustification.md`](../039-artifact-and-crate-boundary-rejustification.md)
Strict lane: [`../../../specs/075-artifact-and-crate-boundary-review-strict-lane.md`](../../../specs/075-artifact-and-crate-boundary-review-strict-lane.md)
Contract: [`../../../contracts/031-artifact-and-crate-boundary-contract.md`](../../../contracts/031-artifact-and-crate-boundary-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Review small crates for current ownership clarity rather than line count.

## Acceptance

- retained small crates have concise ownership justification
- merge candidates are evidence-backed or explicitly absent

## Outcome

- reviewed current workspace crate sizes and source shapes
- retained small crates by ownership rather than line count:
  `effigy-core`, `effigy-runtime-plan`, `effigy-process`, `effigy-routing`,
  `effigy-gateway`, and `effigy-ui`
- found no immediate crate merge candidate with enough ownership evidence

## Validation

- `find crates -maxdepth 2 -name Cargo.toml | sort`
- crate size/source inventory review
- package-map update in `689`

## Next Task

Execute `689` to refresh package-map and crate-boundary docs.
