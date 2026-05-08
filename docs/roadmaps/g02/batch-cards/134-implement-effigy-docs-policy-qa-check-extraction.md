# 134 Implement Effigy Docs-Policy QA Check Extraction

Status: archived
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the remaining reusable docs QA check cluster out of
`src/runner/docs_command.rs` so link scanning, content/heading/path checks, and
workflow-path validation stop depending on one large runner-owned adapter.

## In Scope

- widen `effigy-docs-policy` around reusable docs QA check ownership
- move the next trustworthy docs QA cluster there
- reconnect the current docs command path without changing user-facing
  behavior
- leave the next modularization batch explicit

## Out Of Scope

- broad docs feature widening
- doctor extraction in the same batch
- release closure

## Acceptance Criteria

- more of the docs QA surface no longer sits entirely in `runner`
- the docs-policy boundary is clearer and more reusable than today
- the next modularization batch is explicit

## Validation

- targeted Rust validation for the moved docs-policy checks
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`135-decide-post-docs-policy-qa-check-extraction-boundary.md`](./135-decide-post-docs-policy-qa-check-extraction-boundary.md)
to classify the remaining docs shell before modularization jumps to the next
domain cluster.
