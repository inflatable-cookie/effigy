# 225 Decide Post Bootstrap Runner Shell Follow Up Cleanup V2 Boundary

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining bootstrap shell in
`src/runner/bootstrap_command.rs` is now honest enough to pause after the
runner-shell cleanup batch.

## In Scope

- inspect what still remains in `bootstrap_command.rs`
- decide whether the remaining shell is now mostly adapter and callback work
- record the decision honestly in the lane surfaces
- set the next ready card only if another bounded bootstrap slice is still
  justified

## Out Of Scope

- release execution
- demo/docs parallel cleanup
- shifting to another seam without recording the bootstrap boundary first

## Acceptance Criteria

- the post-`224` bootstrap boundary is recorded clearly
- the next move is explicit:
  - either bootstrap pauses cleanly
  - or one more bounded bootstrap card is opened

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`226-implement-effigy-bootstrap-integration-test-ownership.md`](./226-implement-effigy-bootstrap-integration-test-ownership.md)
to move the remaining crate-domain bootstrap tests out of the runner shell and
into `crates/effigy-bootstrap/tests/`.
