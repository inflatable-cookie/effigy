# 508 - Move Dump Destination Normalization into Effigy Data

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move pure DB dump destination normalization into `effigy-data`.

## Scope

- add a pure dump destination normalization helper to `effigy-data`
- preserve local relative path joining against cwd
- preserve absolute path passthrough
- preserve `oci://` reference passthrough
- preserve `~` and `~/...` expansion when a home path is supplied
- migrate `resolve_db_dump_output_paths` to the shared helper
- keep the runner responsible for reading `HOME`

## Non-Goals

- no artifact capture/staging migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when DB dump destination normalization is no longer
implemented inside `src/runner/container_command/data.rs`.

## Closeout

Added `normalize_dump_destination_path` to `effigy-data` and migrated
`resolve_db_dump_output_paths` to use it. The runner still reads `HOME`, while
the data crate owns pure destination normalization for relative paths, absolute
paths, `oci://` references, and tilde expansion.

## Validation

- `cargo test -p effigy-data` passed
- `cargo test -p effigy --lib container_command::data` passed
- `git diff --check` passed

## Next Task

Start card
[`509-add-data-artifact-handoff-plan-foundation.md`](./509-add-data-artifact-handoff-plan-foundation.md).
