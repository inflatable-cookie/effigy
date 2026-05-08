# 515 - Add Data Target Selection Plan

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move shared seed/dump target selection rules into `effigy-data`.

## Scope

- add a pure target selection helper for optional requested target names
- support seed and dump operation labels for error text
- preserve single-target default selection
- preserve unknown target errors with valid target listing
- preserve multi-target missing-target errors
- preserve duplicate target rejection
- migrate DB seed target resolution to use the helper
- migrate DB dump target resolution to use the helper

## Non-Goals

- no container service matching migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when DB seed and dump target selection no longer duplicate
the same validation logic in runner modules.

## Closeout

Added `select_data_targets` and `DataTargetSelectionError` to `effigy-data`.
DB seed and container data dump now delegate optional target selection,
single-target defaulting, unknown-target checks, missing-target checks, and
duplicate-target checks to the shared helper while preserving existing
runner-owned error wording.

## Validation

- `cargo test -p effigy-data` passed before the service-selection follow-up
- `cargo test -p effigy --lib container_command::data` passed
- `cargo test -p effigy --lib runner::db_seed::tests` passed after the
  service-selection follow-up
- `cargo check -p effigy --lib` passed after the service-selection follow-up
  with the existing `runtime_activation_report_for_result` dead-code warning

## Next Task

Start card
[`516-add-data-service-selection-plan-foundation.md`](./516-add-data-service-selection-plan-foundation.md).
