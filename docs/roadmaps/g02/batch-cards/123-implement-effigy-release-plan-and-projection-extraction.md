# 123 Implement Effigy Release Plan And Projection Extraction

Status: archived
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the heavier release plan and projection ownership out of
`src/runner/release_command.rs` now that `effigy-release` owns config, gates,
status, and the simpler result projections.

## In Scope

- move the next trustworthy release plan or projection contracts into
  `effigy-release`
- reconnect the current runtime path without changing user-facing behavior
- leave the next release extraction batch explicit

## Out Of Scope

- release execution
- broad release TUI/menu extraction
- consumer rollout work

## Acceptance Criteria

- more of the release plan/projection surface no longer sits entirely inside
  `runner`
- `effigy-release` owns a wider release-facing API than result summaries alone
- the next extraction batch is explicit

## Validation

- targeted Rust validation for the moved release contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Open the next extraction batch using the widened `effigy-release` boundary,
likely release state persistence and execution orchestration before the
modularization boundary decision and `g02.007` resume.
