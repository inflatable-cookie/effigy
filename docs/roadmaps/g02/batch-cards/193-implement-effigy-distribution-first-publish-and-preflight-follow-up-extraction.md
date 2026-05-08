# 193 Implement Effigy Distribution First Publish And Preflight Follow Up Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the remaining coherent distribution-domain layer out of
`src/runner/distribution_command.rs` around first-publish and preflight
orchestration.

## In Scope

- widen `effigy-distribution` beyond metadata, summary, and closeout ownership
- target the remaining reusable distribution cluster around:
  - preflight shaping
  - first-publish orchestration
  - publish-cycle summary/result shaping
- reduce `src/runner/distribution_command.rs` materially again
- update lane state and currentness surfaces honestly

## Out Of Scope

- release closure
- container-lane design work
- generic CLI shell/help cleanup outside distribution

## Acceptance Criteria

- `src/runner/distribution_command.rs` no longer owns the bulk of the
  first-publish/preflight distribution-domain cluster
- the remaining distribution shell is described honestly after the batch
- the next move is a boundary decision, not another guessed slice

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`194-decide-post-distribution-first-publish-and-preflight-follow-up-boundary.md`](./194-decide-post-distribution-first-publish-and-preflight-follow-up-boundary.md)
to decide whether the remaining distribution shell is finally honest enough to
pause.
