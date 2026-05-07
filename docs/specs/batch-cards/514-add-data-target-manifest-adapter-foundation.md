# 514 - Add Data Target Manifest Adapter Foundation

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Add a small manifest adapter layer so logical data target collection can move
toward `effigy-data` without making that crate depend on `effigy-manifest`.

## Scope

- add dependency-light input structs to `effigy-data` for manifest bundle/data
  target material
- add a pure target collection helper that produces `ResolvedDataTarget`
- preserve existing precedence: explicit `[data.targets]` entries replace
  bundle-derived targets with the same name
- migrate `logical_database_targets` in runner to build adapter inputs and call
  the shared helper
- keep manifest parsing and TOML access in runner

## Non-Goals

- no seed/dump validation migration yet
- no container service matching migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when target collection rules are tested in `effigy-data`
and runner `logical_database_targets` delegates to the shared collector.

## Closeout

Added manifest-neutral target adapter inputs and `collect_manifest_data_targets`
to `effigy-data`. The runner still reads `effigy-manifest` and TOML values, but
target collection precedence and filtering now live in the data crate.

## Validation

- `cargo test -p effigy-data` passed
- `cargo test -p effigy --lib db_seed` passed
- `cargo test -p effigy --lib container_command::data` passed
- `git diff --check` passed

## Next Task

Start card
[`515-add-data-target-selection-plan.md`](./515-add-data-target-selection-plan.md).
