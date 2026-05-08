# 191 Implement Effigy Distribution Metadata And Closeout Follow Up Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next coherent distribution-domain layer out of
`src/runner/distribution_command.rs` now that the artifact/log execution slice
is crate-owned.

## In Scope

- widen `effigy-distribution` beyond artifact/log execution
- target the remaining reusable distribution cluster around:
  - metadata validation
  - preflight shaping
  - summary shaping
  - closeout generation
  - or the first-publish orchestration layer where it forms one coherent slice
- reduce `src/runner/distribution_command.rs` materially again
- update lane state and currentness surfaces honestly

## Out Of Scope

- release closure
- reopening bootstrap
- container-lane design work
- generic CLI shell/help cleanup outside distribution

## Acceptance Criteria

- `src/runner/distribution_command.rs` no longer owns the bulk of the chosen
  follow-up distribution-domain cluster
- the remaining distribution shell is described honestly after the batch
- the next move is a boundary decision, not another guessed slice

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`192-decide-post-distribution-metadata-and-closeout-follow-up-boundary.md`](./192-decide-post-distribution-metadata-and-closeout-follow-up-boundary.md)
to decide whether the remaining distribution shell is now honest enough to
pause or still needs one more bounded extraction.
