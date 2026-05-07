# 167 Implement Effigy Demo Record And Projection Follow-up Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next reusable demo-domain record/projection slice so
`src/runner/demo_command.rs` stops owning the shared demo record model and its
projection helpers directly.

## In Scope

- identify one bounded record/projection cluster inside `src/runner/demo_command.rs`
- move that cluster into `crates/effigy-demo`
- reconnect runner code as a thinner adapter over the extracted model/API
- leave text rendering and heavy process orchestration explicit if they do not
  fit this batch

## Out Of Scope

- full `demo_command.rs` decomposition in one batch
- release-lane execution
- unrelated TUI or release cleanup

## Acceptance Criteria

- `crates/effigy-demo` widens beyond active-state ownership into record/projection ownership
- `src/runner/demo_command.rs` shrinks or thins again in a meaningful way
- the remaining demo-runner shell is clearer after the batch

## Validation

- bounded demo-domain validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`168-decide-post-demo-record-and-projection-follow-up-boundary.md`
to decide whether the remaining demo runner shell is finally honest enough or
still needs another bounded `effigy-demo` extraction batch.
