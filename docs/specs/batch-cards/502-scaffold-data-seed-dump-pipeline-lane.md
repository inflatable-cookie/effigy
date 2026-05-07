# 502 - Scaffold Data Seed Dump Pipeline Lane

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Open the `g04.005` data seed/dump pipeline lane and select the first
implementation slice.

## Scope

- mark `g04.004` closed and `g04.005` active in front doors
- create the `047` strict lane
- inventory data seed/dump ownership hotspots
- select the first bounded `effigy-data` foundation slice
- do not implement code in this scaffold card

## Non-Goals

- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the first `g04.005` implementation card is ready.

## Ownership Inventory

Current data seed/dump ownership is concentrated in four files:

- `src/runner/container_command/data.rs`: 1645 lines; owns container data seed
  and dump command orchestration, prompt policy, dump destination expansion,
  logical target resolution, service selection, postgres/mariadb dump command
  rendering, OCI capture handling, and tests.
- `src/runner/db_seed.rs`: 1256 lines; owns bootstrap DB seed prompt handling,
  seed path expansion, artifact staging and OCI pull handoff, compatibility env
  creation, logical target resolution, runtime activation, task dispatch, and
  builtin postgres/mariadb import/reset command rendering.
- `src/runner/artifact_command.rs`: 801 lines; owns CLI artifact staging,
  capture, OCI transport adapter use, kind parsing, and output rendering.
- `crates/effigy-artifacts/src/lib.rs`: 988 lines; owns artifact references,
  local/OCI staging, adapter traits, metadata, reports, and tests.

The first extraction should avoid side effects and runner command migration.
The clean slice is a dependency-light `effigy-data` crate with data target,
database kind, seed input, dump destination, command plan, artifact handoff, and
operation report models. Runner modules can then migrate resolution and command
rendering behind those types in later cards.

## Closeout

`g04.005` is active and the first implementation slice is ready as card `503`.
No code changed in this scaffold card.

## Validation

- `git diff --check`

## Next Task

Start card
[`503-scaffold-effigy-data-crate-and-target-model.md`](./503-scaffold-effigy-data-crate-and-target-model.md).
