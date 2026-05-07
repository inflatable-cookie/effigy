# 222 Implement Effigy Distribution Runner Shell Follow Up Cleanup V2

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Reduce the remaining distribution runner shell now that contracts is paused and
the next best disjoint `/src` seam is distribution.

## In Scope

- inspect the remaining `src/runner/distribution_command.rs` shell
- move one meaningful runner-owned distribution cluster into
  `crates/effigy-distribution`
- leave runner-local only honest CLI entry, final render choice, and error
  mapping where possible

## Out Of Scope

- release execution
- demo/docs/container cleanup
- speculative new crate work outside `effigy-distribution`

## Acceptance Criteria

- `src/runner/distribution_command.rs` gets materially smaller
- one real runner-owned distribution shell cluster moves into
  `effigy-distribution`
- the next move is a boundary decision, not another guessed slice

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`223-decide-post-distribution-runner-shell-follow-up-cleanup-v2-boundary.md`](./223-decide-post-distribution-runner-shell-follow-up-cleanup-v2-boundary.md)
to decide whether the remaining distribution shell now pauses cleanly.
