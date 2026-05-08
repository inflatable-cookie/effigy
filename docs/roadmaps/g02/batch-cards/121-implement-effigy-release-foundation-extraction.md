# 121 Implement Effigy Release Foundation Extraction

Status: archived
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Start extracting the `release_command.rs` surface into a real `effigy-release`
crate now that the first release-cluster policy ownership is out of `runner`.

## In Scope

- create the first `effigy-release` workspace crate
- move the first trustworthy release-facing contracts or services out of
  `runner`
- reconnect the current runtime path without changing user-facing behavior
- leave the next release extraction batch explicit

## Out Of Scope

- release execution
- broad container or distribution widening beyond the new public APIs
- consumer rollout work

## Acceptance Criteria

- `effigy-release` exists and is used by the main crate
- part of the release surface no longer sits entirely inside `runner`
- the next extraction batch is explicit

## Validation

- targeted Rust validation for the moved release contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Open the next extraction batch using the new `effigy-release` boundary, likely
release state and projection ownership before deeper release execution movement
or the modularization pause decision before `g02.007` resumes.
