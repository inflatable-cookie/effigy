# 512 - Add Seed Artifact Staging Plan Foundation

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move pure DB seed artifact staging intent into `effigy-data`.

## Scope

- add a dependency-light seed artifact staging plan type to `effigy-data`
- plan local source paths relative to repo root when needed
- plan the local artifact root
- plan the OCI pull destination root
- migrate DB seed staging runner code to consume the plan
- keep artifact reference parsing, file readability checks, local staging, OCI
  pull, OCI staging, and error rendering in the runner

## Non-Goals

- no `effigy-artifacts` dependency in `effigy-data`
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when DB seed staging path/root decisions are owned by
`effigy-data`.

## Closeout

Added `SeedArtifactStagingPlan` and `seed_artifact_staging_plan` to
`effigy-data`. DB seed staging now gets local source paths, artifact roots, and
OCI pull destination roots from the data crate while keeping artifact parsing,
file checks, staging, OCI pull, and OCI staging in the runner.

## Validation

- `cargo test -p effigy-data` passed
- `cargo test -p effigy --lib db_seed` passed
- `git diff --check` passed

## Next Task

Start card
[`513-close-data-pipeline-foundation-pass.md`](./513-close-data-pipeline-foundation-pass.md).
