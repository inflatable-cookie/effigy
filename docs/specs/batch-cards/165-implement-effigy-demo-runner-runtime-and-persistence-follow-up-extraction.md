# 165 Implement Effigy Demo Runner Runtime And Persistence Follow-up Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next meaningful `effigy-demo` runner slice so
`src/runner/demo_command.rs` stops owning as much demo runtime and persistence
logic directly.

## In Scope

- identify one bounded demo runner cluster inside `src/runner/demo_command.rs`
- move that cluster into `crates/effigy-demo`
- reconnect runner code as a thinner adapter over the extracted API
- leave broader demo rendering and command orchestration explicit if they do
  not fit this batch

## Out Of Scope

- full `demo_command.rs` decomposition in one batch
- release-lane execution
- unrelated TUI or release cleanup

## Acceptance Criteria

- `crates/effigy-demo` widens beyond browser/history/runtime model ownership
- `src/runner/demo_command.rs` shrinks or thins again in a meaningful way
- the next remaining demo-runner shell is explicit after the batch

## Validation

- bounded demo-domain validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`166-decide-post-demo-runner-runtime-and-persistence-follow-up-boundary.md`
to decide whether the remaining demo runner shell is honest enough or still
needs another bounded `effigy-demo` extraction batch.
