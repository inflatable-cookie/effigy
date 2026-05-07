# 118 Implement Effigy Tasks Foundation Extraction

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Start the first real domain extraction by moving task-routing and
task-execution foundation out of the binary shell / `runner` knot and into a
dedicated `effigy-tasks` crate built on `effigy-core`.

## In Scope

- create the `effigy-tasks` crate
- move the first trustworthy task-domain contracts there:
  - task context and resolution types
  - catalog and selector foundation where it moves cleanly
  - task-routing and manifest-task execution helpers that belong to the domain
- reconnect the current shell/runtime path to use the extracted crate without
  changing user-facing behavior
- leave the next extraction batch explicit

## Out Of Scope

- full release/distribution/container extraction
- broad TUI or demo movement
- release execution
- consumer rollout work

## Acceptance Criteria

- `effigy-tasks` is real and used by the main crate
- the moved task-domain code reduces `runner` ownership in a meaningful way
- the next extraction batch is explicit

## Validation

- targeted Rust validation for the moved task-domain contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Open the next extraction batch based on the result, likely manifest/core
follow-up or the release-blocking container/distribution/release cluster.
