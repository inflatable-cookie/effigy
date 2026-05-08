# 116 Decide Domain Crate Boundaries And Rhai API Contract

Status: archived
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Classify Effigy's major product domains into a trustworthy first crate-boundary
plan and define the Rust API / Rhai adapter contract that later extraction
batches should follow.

## In Scope

- inventory the current major domains inside Effigy
- decide which domains merit separate crates first and which should stay in the
  shell or backbone for now
- define dependency direction between backbone and domain crates
- define the API shape each extracted domain should expose to CLI/runtime code
  and to Rhai
- position the next extraction batch explicitly

## Out Of Scope

- broad code movement across the repo
- mechanical crate extraction without first-class boundaries
- release execution
- consumer rollout work

## Acceptance Criteria

- the first modularization plan names the major domains honestly
- the crate-boundary and dependency rules are explicit enough to guide code
  extraction
- the Rust API / Rhai adapter rule is explicit enough to stop future feature
  work from hardcoding around the old runtime shape
- the next extraction batch is explicit

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Open the first implementation batch for the highest-value extraction slice
justified by the decided crate boundaries.
