# 117 Implement Workspace And Effigy Core Foundation

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Establish the Cargo workspace and first shared `effigy-core` backbone crate so
later domain extraction batches can move product logic out of the binary shell
without inventing one-off dependency paths.

## In Scope

- convert the repo to a workspace shape that can host multiple library crates
- create the initial `effigy-core` crate
- move the first agreed backbone contracts into `effigy-core`:
  - command model types where justified
  - shared repo/context resolution contracts
  - shared error/output contracts where they can move cleanly
  - manifest loading/composition contracts that other domains will need
- reconnect the current binary/runtime shell to the new backbone without
  changing user-facing behavior
- leave the next domain extraction batch explicit

## Out Of Scope

- full extraction of tasks, release, distribution, containers, or demos
- broad behavior changes
- release execution
- consumer rollout work

## Acceptance Criteria

- the repo is a real Cargo workspace, not a planned future shape
- `effigy-core` exists and owns meaningful shared runtime contracts
- the remaining binary/runtime shell is thinner in a real way
- the next domain extraction batch is explicit

## Validation

- targeted Rust validation for the moved backbone contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Open the first domain extraction batch that builds on `effigy-core`, likely
`effigy-tasks` or the release-blocking container/distribution/release cluster.
