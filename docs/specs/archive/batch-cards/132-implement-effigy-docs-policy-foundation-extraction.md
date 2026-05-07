# 132 Implement Effigy Docs-Policy Foundation Extraction

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the next clearly reusable docs-policy slice out of
`src/runner/docs_command.rs` so docs QA and index-policy behavior stop
depending on one large runner-owned adapter.

## In Scope

- add the first `effigy-docs-policy` workspace crate
- move the next trustworthy docs-policy ownership there
- reconnect the current docs command path without changing user-facing
  behavior
- leave the next modularization batch explicit

## Out Of Scope

- broad docs feature widening
- doctor extraction in the same batch
- release closure

## Acceptance Criteria

- more of the docs-policy surface no longer sits entirely in `runner`
- the docs-policy boundary is clearer and more reusable than today
- the next modularization batch is explicit

## Validation

- targeted Rust validation for the moved docs-policy contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`133-decide-post-docs-policy-foundation-extraction-boundary.md`](./133-decide-post-docs-policy-foundation-extraction-boundary.md)
to classify the remaining docs shell before modularization jumps to the next
domain cluster.
