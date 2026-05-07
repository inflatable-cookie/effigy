# 047 - Data Seed Dump Pipeline Strict Lane

Roadmap: [`g04.005`](../roadmaps/g04/005-data-seed-dump-pipeline.md)

Status: Active
Owner: Platform
Created: 2026-05-07

## Purpose

Untangle database seed/dump/artifact capture planning from container command
glue.

`g04.004` moved container operation surfaces behind typed operation plans and
manager-owned backend invocation plans. The next pressure point is data
planning: DB target resolution, seed source normalization, dump destination
selection, artifact staging/capture, and database command rendering still live
inside runner command modules.

## Hard Boundaries

- preserve `--db-seed`, `--db-dump`, and `oci://` behavior
- keep actual task dispatch/container exec in runner/runtime pipeline
- no public CLI behavior changes unless a card explicitly selects a cleanup
  break
- no release work
- no `.github/workflows/` edits

## Current Ready Card

[`509-add-data-artifact-handoff-plan-foundation.md`](./batch-cards/509-add-data-artifact-handoff-plan-foundation.md)

## Execution Chain

- `501` complete: close final `g04.004` runner helper drift
- `502` complete: scaffold data seed/dump pipeline lane
- `503` complete: scaffold `effigy-data` crate and target model
- `504` complete: move database command rendering into `effigy-data`
- `505` complete: centralize data artifact reference classification
- `506` complete: move logical data target model into `effigy-data`
- `507` complete: move seed source normalization into `effigy-data`
- `508` complete: move dump destination normalization into `effigy-data`
- `509` ready: add data artifact handoff plan foundation

## Exit Condition

This lane closes when data seed/dump planning has moved behind dependency-light
request/plan/report types and runner data modules no longer own DB target
resolution or database command rendering.

## Next Task

Card
[`509-add-data-artifact-handoff-plan-foundation.md`](./batch-cards/509-add-data-artifact-handoff-plan-foundation.md).
