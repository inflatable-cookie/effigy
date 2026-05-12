# 056 - Data Seed Dump Plan Consumption Strict Lane

Roadmap: [`g04.014`](../roadmaps/g04/014-data-seed-dump-plan-consumption.md)

Status: Active
Owner: Platform
Created: 2026-05-07

## Purpose

Make seed and dump execution consume `effigy-data` plan structs instead of
reassembling target, artifact, and command decisions in runner glue.

## Hard Boundaries

- no release work
- no `.github/workflows/` edits
- preserve public CLI behavior and existing seed/dump syntax
- keep prompting and operator rendering in runner modules
- keep actual artifact transport in existing artifact adapters

## Current Ready Card

[`584-wire-container-data-dump-through-data-dump-plan.md`](./batch-cards/584-wire-container-data-dump-through-data-dump-plan.md)

## Execution Chain

- `582` complete: wire bootstrap DB seed through `DataSeedPlan`
- `583` complete: confirm container data seed shares the seed-plan path
- `584` ready: wire container data dump through `DataDumpPlan`

## Focus

- make bootstrap `--db-seed` build one seed plan before execution
- then migrate `container data seed`
- then migrate dump planning, including `oci://` destinations and `--push`
- split `effigy-data` only after runner plan consumption is real

## Exit Condition

This lane closes when seed and dump execution paths consume full plan structs
and the remaining runner code is prompt/render/dispatch glue.

## Next Task

Card
[`584-wire-container-data-dump-through-data-dump-plan.md`](./batch-cards/584-wire-container-data-dump-through-data-dump-plan.md).
