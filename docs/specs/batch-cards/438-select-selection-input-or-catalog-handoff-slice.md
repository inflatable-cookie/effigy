# 438 - Select Selection Input or Catalog Handoff Slice

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose whether the next `g04.002` slice should move task selection input
planning or first introduce a catalog handoff shape.

## Scope

- review the new discovery plan boundary
- inspect `LoadedCatalog`, `TaskSelection`, and routing selection lifetimes
- decide whether a lifetime-light selection input can live in
  `effigy-execution` now
- create the next smallest implementation card

## Non-Goals

- no binding migration
- no runtime activation migration
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the next bounded implementation card is ready and
the lane/front-door docs point to it.

## Decision

Move selection input/result summaries next, not catalog handoff.

Do not introduce a catalog handoff shape yet. `LoadedCatalog` owns parsed
manifest data, defer metadata, bundle root, catalog root, manifest path, and
composition depth. Duplicating that in `effigy-execution` would either create a
second manifest model or force `effigy-execution` to depend on the manifest and
routing crates too early.

The cleaner slice is lifetime-light selection planning:

- `effigy-execution` owns `ExecutionSelectionInput`
- runner still calls `select_catalog_and_task`
- runner converts the selected catalog/task metadata into an
  `ExecutionSelectionPlan` summary
- borrowed `TaskSelection<'a>` stays runner-owned for actual dispatch
- fallback surfaces still use runner preflight until the summary proves useful

This gives the architecture a typed selection boundary without moving borrowed
manifest references across crate boundaries.

## Next Implementation Slice

Card `439` should add selection input/result summary types and wire them into
runner selection.

The card should avoid moving:

- `LoadedCatalog`
- `TaskSelection<'a>`
- fallback built-in/exec-alias/deferral execution
- managed/standard dispatch

Expected follow-on after `439`: decide whether binding input can be planned
from the selection summary, or whether runner needs one more selected-task
adapter first.

## Validation

- docs path check for updated spec and roadmap front doors
- `git diff --check`

## Closeout

Selected lifetime-light selection input/result summaries as the next
implementation slice and created card `439`.

## Next Task

Start card
[`439-add-execution-selection-plan-summary.md`](./439-add-execution-selection-plan-summary.md).
