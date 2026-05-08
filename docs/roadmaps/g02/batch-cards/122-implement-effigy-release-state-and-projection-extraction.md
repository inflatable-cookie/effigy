# 122 Implement Effigy Release State And Projection Extraction

Status: archived
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the next trustworthy slice of release ownership out of
`src/runner/release_command.rs` now that `effigy-release` owns config
resolution and gate execution.

## In Scope

- move the first release state, plan, or projection contracts that now sit
  cleanly on top of `effigy-release`
- reconnect the current runtime path without changing user-facing behavior
- leave the next release extraction batch explicit

## Out Of Scope

- release execution
- broad release TUI/menu extraction
- consumer rollout work

## Acceptance Criteria

- more of the release surface no longer sits entirely inside `runner`
- `effigy-release` owns more than config and gate primitives
- the next extraction batch is explicit

## Validation

- targeted Rust validation for the moved release contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Open the next extraction batch using the widened `effigy-release` boundary,
likely the heavier release plan and projection cluster before deeper
orchestration movement or the modularization boundary decision before
`g02.007` resumes.
