# 432 - Scaffold Execution Pipeline Ownership Lane

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-07

## Goal

Open the implementation lane for `g04.002` and define the first safe execution
pipeline ownership slice.

## Scope

- create `docs/specs/044-execution-pipeline-ownership-strict-lane.md`
- inventory the current direct, bootstrap, Rhai, run-array, demo, deferral, and
  managed execution request paths
- define the first `effigy-execution` planning types to add
- decide which pure preflight data can move first without changing side effects
- create the first implementation card for the selected slice

## Non-Goals

- no implementation code changes in this scaffold card
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `g04.002` has a strict lane, a first implementation
card, and a bounded migration order that does not require guessing.

## Next Task

Create the first execution pipeline implementation card selected by the lane.
