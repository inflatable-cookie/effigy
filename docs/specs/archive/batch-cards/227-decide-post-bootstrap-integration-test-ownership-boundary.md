# 227 Decide Post Bootstrap Integration Test Ownership Boundary

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining bootstrap shell in
`src/runner/bootstrap_command.rs` is now honest enough to pause after the
integration test ownership move.

## In Scope

- inspect what still remains in `bootstrap_command.rs`
- decide whether the remaining shell is now mostly adapter, callback, and
  runner-path integration work
- record the decision honestly in the lane surfaces
- set the next ready card only if another bounded bootstrap slice is still
  justified

## Out Of Scope

- release execution
- demo/docs parallel cleanup
- shifting to another seam without recording the bootstrap boundary first

## Acceptance Criteria

- the post-`226` bootstrap boundary is recorded clearly
- the next move is explicit:
  - either bootstrap pauses cleanly
  - or one more bounded bootstrap card is opened

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`228-decide-next-src-shell-cleanup-priority-after-bootstrap-pause-boundary.md`](./228-decide-next-src-shell-cleanup-priority-after-bootstrap-pause-boundary.md)
to pick the next `/src` cleanup priority after pausing bootstrap.
