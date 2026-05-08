# 120 Implement Release Cluster Foundation Extraction

Status: archived
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Start extracting the release-blocking container, distribution, and release
cluster out of `runner` now that `effigy-core`, `effigy-tasks`, and
`effigy-manifest` are real.

## In Scope

- define the first shared foundation for the release-blocking cluster
- move the first trustworthy container/distribution/release contracts or
  services out of `runner`
- reconnect the current runtime path without changing user-facing behavior
- leave the next extraction batch explicit

## Out Of Scope

- release execution
- broad demo extraction
- consumer rollout work

## Acceptance Criteria

- the release-blocking cluster no longer sits entirely inside `runner`
- the extraction reduces the remaining architecture debt that blocks `v0.3`
- the next extraction batch is explicit

## Validation

- targeted Rust validation for the moved cluster contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Open the next extraction batch based on the result, likely the first dedicated
`effigy-release` extraction now that container and distribution policy
ownership no longer sit entirely inside `runner`.
