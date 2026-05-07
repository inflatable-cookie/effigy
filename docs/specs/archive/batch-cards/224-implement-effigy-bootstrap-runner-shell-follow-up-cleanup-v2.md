# 224 Implement Effigy Bootstrap Runner Shell Follow Up Cleanup V2

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Reduce the remaining bootstrap runner shell now that distribution is paused and
bootstrap is the next best disjoint `/src` seam.

## In Scope

- inspect the remaining `src/runner/bootstrap_command.rs` shell
- move one meaningful runner-owned bootstrap cluster into
  `crates/effigy-bootstrap`
- remove crate-adoption residue that still leaves duplicate or dead bootstrap
  helpers in the runner
- leave runner-local only honest CLI entry, callback wiring, final render
  choice, and error mapping where possible

## Out Of Scope

- release execution
- demo/docs/container parallel cleanup
- speculative new crate work outside `effigy-bootstrap`

## Acceptance Criteria

- `src/runner/bootstrap_command.rs` gets materially smaller
- one real runner-owned bootstrap shell cluster moves into `effigy-bootstrap`
- dead or duplicate bootstrap helper residue from the crate adoption is removed
- the next move is a boundary decision, not another guessed slice

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`225-decide-post-bootstrap-runner-shell-follow-up-cleanup-v2-boundary.md`](./225-decide-post-bootstrap-runner-shell-follow-up-cleanup-v2-boundary.md)
to decide whether bootstrap can now pause on an honest shell boundary.
