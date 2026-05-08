# 510 - Wire Data Artifact Handoff Plans into Runner Glue

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Use `effigy-data` artifact handoff plans in DB seed and container data dump
runner code.

## Scope

- migrate DB seed artifact staging dispatch to use `seed_artifact_handoff`
- migrate container data dump OCI capture dispatch to use
  `dump_artifact_handoff`
- remove runner-local dump capture source path planning
- preserve all artifact transport, OCI pull, OCI push, file IO, and output
  rendering in runner modules

## Non-Goals

- no public CLI behavior changes
- no artifact transport side effects in `effigy-data`
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when runner data seed/dump code consumes the shared
artifact handoff intent instead of planning it locally.

## Closeout

DB seed staging now dispatches through `seed_artifact_handoff`, and container
data dump capture now dispatches through `dump_artifact_handoff`. Runner modules
still own actual artifact transport, file IO, and output rendering. The
runner-local dump capture source path planner was removed.

## Validation

- `cargo test -p effigy-data` passed
- `cargo test -p effigy --lib container_command::data` passed
- `cargo test -p effigy --lib db_seed` passed
- `git diff --check` passed

## Next Task

Start card
[`511-select-artifact-staging-migration-or-foundation-closeout.md`](./511-select-artifact-staging-migration-or-foundation-closeout.md).
