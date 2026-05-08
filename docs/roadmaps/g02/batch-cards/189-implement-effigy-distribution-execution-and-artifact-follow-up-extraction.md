# 189 Implement Effigy Distribution Execution And Artifact Follow Up Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next real distribution-domain layer out of
`src/runner/distribution_command.rs` so the remaining `/src` shell gets
cleaner without reopening already-paused demo, release, or bootstrap seams.

## In Scope

- widen `effigy-distribution` beyond policy-only ownership
- move a bounded distribution execution/artifact layer out of
  `src/runner/distribution_command.rs`
- target the reusable cluster around artifact validation, GLIBC inspection,
  first-publish execution helpers, summary shaping, or closeout shaping where
  it forms one coherent domain slice
- reduce `src/runner/distribution_command.rs` materially
- update lane state and currentness surfaces honestly

## Out Of Scope

- release closure
- reopening bootstrap
- container-lane design work or container runtime widening
- generic CLI shell/help cleanup outside distribution

## Acceptance Criteria

- `src/runner/distribution_command.rs` no longer owns the bulk of the chosen
  distribution-domain cluster
- the remaining distribution shell is described honestly after the batch
- the next move is a boundary decision, not another guessed follow-up slice

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`190-decide-post-distribution-execution-and-artifact-follow-up-boundary.md`](./190-decide-post-distribution-execution-and-artifact-follow-up-boundary.md).
