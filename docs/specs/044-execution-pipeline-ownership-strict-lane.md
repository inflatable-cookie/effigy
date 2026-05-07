# 044 - Execution Pipeline Ownership Strict Lane

Roadmap: [`g04.002`](../roadmaps/g04/002-execution-pipeline-ownership.md)

Status: Complete
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

None. This lane is complete.

## Execution Chain

- `432` complete: scaffold execution pipeline ownership lane
- `433` complete: add execution dispatch plan foundation
- `434` complete: select next execution planning slice
- `435` complete: move execution preflight input behind dispatch plan
- `436` complete: select discovery or selection planning slice
- `437` complete: add execution discovery plan foundation
- `438` complete: select selection input or catalog handoff slice
- `439` complete: add execution selection plan summary
- `440` complete: select binding input or selected-task adapter slice
- `441` complete: add execution binding plan summary
- `442` complete: select dispatch stage or runtime activation handoff
- `443` ready: close execution pipeline ownership and hand off runtime activation
- `443` complete: close execution pipeline ownership and hand off runtime activation

## Exit Condition

This lane closes when `run_manifest_task_request` consumes a resolved execution
plan, embedded task dispatch cannot bypass request construction, and standard
plus managed execution pipelines either shrink below the agreed threshold or
split into clear owner modules.

## Closeout

Execution planning now has typed shared surfaces for dispatch, preflight,
runtime args, discovery, selection summary, and binding summary. Standard and
managed pipeline file-size targets remain open because their remaining bulk is
runtime/container ownership, handed to `g04.003`.

## Next Task

Card
[`444-scaffold-runtime-activation-pipeline-lane.md`](./batch-cards/444-scaffold-runtime-activation-pipeline-lane.md).
