# 186 Implement Effigy Bootstrap Foundation Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the first real bootstrap-domain boundary out of
`src/runner/bootstrap_command.rs` so `/src` gets cleaner without reopening
already-paused demo or release shell seams.

## In Scope

- add a real bootstrap workspace crate or equivalent promoted library boundary
- move bootstrap request resolution and execution contracts out of
  `src/runner/bootstrap_command.rs`
- move the git checkout / child-bootstrap / start-task orchestration layer
  where it belongs if that can be done without dragging generic CLI shell work
  into the crate
- reduce `src/runner/bootstrap_command.rs` materially
- update lane state and currentness surfaces honestly

## Out Of Scope

- release closure
- reopening demo or release shell cleanup
- generic CLI shell/help cleanup outside bootstrap

## Acceptance Criteria

- `src/runner/bootstrap_command.rs` no longer owns the bulk of bootstrap-domain
  request/execution logic
- the remaining bootstrap shell is described honestly after the batch
- the next move is a boundary decision, not another guessed follow-up slice

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`187-decide-post-bootstrap-foundation-extraction-boundary.md`](./187-decide-post-bootstrap-foundation-extraction-boundary.md).
