# 181 Implement Effigy Changelog Workspace Extraction And Release Adoption

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the changelog parsing/formatting surface into its own workspace crate
so release prep and changelog commands stop depending on a root-crate module.

## In Scope

- create a real changelog workspace crate from the current `src/changelog.rs`
  surface and its submodules
- reconnect release and changelog command paths to the extracted crate
- reduce root-crate changelog ownership materially
- update lane state and currentness surfaces honestly

## Out Of Scope

- release closure
- unrelated cleanup outside changelog and its immediate adopters
- full release shell decomposition in the same batch

## Acceptance Criteria

- changelog parsing/formatting/validation no longer lives only in the root
  crate
- release prep uses the extracted changelog API
- the remaining release shell is described honestly after the batch

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `182-decide-post-changelog-workspace-extraction-boundary.md`.
