# 012 Decide Demo Runner Foundation Implementation Slice

Status: complete
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Define the first bounded Effigy implementation slice for the demo runner
foundation.

## In Scope

- decide which demo capabilities must ship in the first product slice
- decide which CLI/inspection surfaces belong in that slice
- decide what receipt/artifact normalization must exist before migration work
- keep the slice small enough to implement without pulling in browser polish or
  project-specific migration work

## Out Of Scope

- implementation work in this batch
- Signal migration itself
- TUI widget/layout work
- desktop-client decisions

## Acceptance Criteria

- the roadmap states one bounded first implementation slice for demo runner
  foundation work
- the slice is justified against the Signal reconciliation, not abstract desire
- the next card can move into execution planning or implementation without
  reopening the model

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the slice becomes a grab-bag of browser, runner, and migration work
- the batch starts redesigning the settled model instead of sequencing it

## Next Task

Open the first execution card for demo registry loading, list/inspect surfaces,
and normalized latest-attempt state.
