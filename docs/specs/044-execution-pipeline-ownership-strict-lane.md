# 044 - Execution Pipeline Ownership Strict Lane

Roadmap: [`g04.002`](../roadmaps/g04/002-execution-pipeline-ownership.md)

Status: Active
Owner: Platform
Created: 2026-05-07

## Purpose

Make `effigy-execution` the real execution planning authority.

This lane starts by turning the current runner-owned execution preflight,
binding, routing, and dispatch seams into an explicit migration sequence. The
first implementation slice must be small enough to prove plan ownership without
changing task behavior.

## Hard Boundaries

- no public CLI behavior changes unless a card explicitly selects a cleanup
  break
- no release work
- no `.github/workflows/` edits
- do not move side-effectful runtime/container execution before pure plan types
  exist
- do not bypass `TaskExecutionRequestBuilder` in new embedded dispatch paths

## Current Ready Card

[`434-select-next-execution-planning-slice.md`](./batch-cards/434-select-next-execution-planning-slice.md)

## Execution Chain

- `432` complete: scaffold execution pipeline ownership lane
- `433` complete: add execution dispatch plan foundation
- `434` ready: select next execution planning slice

## Exit Condition

This lane closes when `run_manifest_task_request` consumes a resolved execution
plan, embedded task dispatch cannot bypass request construction, and standard
plus managed execution pipelines either shrink below the agreed threshold or
split into clear owner modules.

## Next Task

Card
[`434-select-next-execution-planning-slice.md`](./batch-cards/434-select-next-execution-planning-slice.md).
