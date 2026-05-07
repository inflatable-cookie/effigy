# 124 Implement Effigy Release State Persistence And Orchestration Extraction

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the remaining release state persistence and execution-oriented ownership
out of `src/runner/release_command.rs` now that `effigy-release` owns the
release-facing config, gate, result, and projection layers.

## In Scope

- move the next trustworthy release persistence or orchestration contracts into
  `effigy-release`
- reconnect the current runtime path without changing user-facing behavior
- leave the next release extraction batch explicit

## Out Of Scope

- release TUI/menu extraction
- final release-lane pause decision
- consumer rollout work

## Acceptance Criteria

- more of the release execution/persistence surface no longer sits entirely in
  `runner`
- `effigy-release` owns a wider release orchestration API than projections
  alone
- the next extraction batch is explicit

## Validation

- targeted Rust validation for the moved release contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `125-decide-post-release-persistence-extraction-boundary.md` to judge
whether `effigy-release` is now broad enough for a modularization boundary
decision or still needs one more execution-side extraction batch.
