# 469 - Decide Runtime Activation Pipeline Closeout

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Decide whether `g04.003` can close after the current activation caller
migrations, or whether one more activation-report/card-level proof slice is
needed.

## Scope

- review `g04.003` exit criteria
- inventory remaining runtime activation bypasses
- choose one next move:
  - close `g04.003` and open `g04.004`
  - add activation report plumbing before closeout
  - add one more caller migration if a real bypass remains
- update roadmap/spec front doors
- do not implement code in this decision card

## Non-Goals

- no public CLI behavior changes
- no broad runtime/container refactor
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the lane has a clear closeout or one final ready
implementation card.

## Closeout

Decision:

- close `g04.003`
- open `g04.004` container operation pipeline

Rationale:

- runtime activation now has typed request, plan, and report surfaces
- exec, DB seed, deferral, standard task, managed task, workspace, bootstrap
  workspace handoff, and Rhai container-sensitive execution all route through
  activation planning
- remaining direct container operation construction belongs to `g04.004`, where
  lifecycle, exec, data, status, logs, stats, shell, and cache operations can be
  moved behind typed operation requests together

## Validation

- `git diff --check`

## Next Task

Scaffold the `g04.004` container operation pipeline lane.
