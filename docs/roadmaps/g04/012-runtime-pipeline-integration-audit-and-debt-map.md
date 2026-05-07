# 012 - Runtime Pipeline Integration Audit And Debt Map

Generation: `g04`

Status: Queued
Owner: Platform
Created: 2026-05-07
Depends on: [`011-contract-promotion-and-closeout.md`](./011-contract-promotion-and-closeout.md)

## Goal

Turn the post-`g04.011` audit into a concrete debt map before doing more
runtime/container refactor work.

## Scope

- inventory every runtime/container path that still bypasses the request,
  plan, manager, data, or artifact seams
- classify each drift-guard allowance as either a real adapter boundary or
  migration debt
- inspect recent container volume and named-volume work and map it into the
  `g04` pipeline model
- inventory duplicated activation-plan construction across runner callers
- inventory `effigy-data` helper use versus full `DataSeedPlan` /
  `DataDumpPlan` consumption
- inventory large new crate files that should be split after integrations
  settle
- select the first implementation lane

## Findings To Confirm

- `RuntimeActivationRoute` exists but callers mostly build task-shaped plans
  without setting an honest route.
- `DataSeedPlan` and `DataDumpPlan` exist, but runner data/seed paths still
  call lower-level helpers directly.
- Some `ContainerOperationPlan` values are built and discarded as proof of
  intent rather than driving execution or reports.
- `container volume list` and named-volume orphan filtering landed after the
  main container-op model and need a formal operation family.
- `qa:architecture` is present but not part of the common QA aggregators.
- New planning crates improved runner hotspots but have become large
  single-file growth points.

## Non-Goals

- no broad implementation refactor in this roadmap
- no release work
- no `.github/workflows/` edits
- no churn-only file splitting

## Acceptance Criteria

- audit table maps every drift allowance to adapter boundary or migration card
- recent volume work has a selected integration path
- first implementation card is ready and scoped
- no stale planning claim says `g04` is closed

## Validation

- `bash scripts/check-runtime-container-drift.sh`
- docs path/link checks for new roadmap/spec/card docs
- `git diff --check`

## Next Task

Open a strict lane and first ready card for this roadmap.
