# 519 - Close Data Seed Dump Pipeline And Open Rhai Lane

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Close `g04.005` and open the next `g04.006` Rhai host API lane.

## Scope

- mark `g04.005` complete
- mark the data strict lane complete
- mark `g04.006` active
- create the `048` Rhai host strict lane
- select the first bounded Rhai host audit/scaffold card
- update roadmap/spec front doors

## Non-Goals

- no Rhai implementation code yet
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the active front doors point to the first `g04.006`
ready card.

## Closeout

Closed the data seed/dump pipeline milestone after moving data target
collection, target selection, service selection, seed/dump source/destination
normalization, database command rendering, artifact handoff planning, and
container data prompt ownership behind narrower modules.

Opened the Rhai host API split lane and selected the first audit/scaffold card.

## Validation

- docs/front-door consistency check passed
- `git diff --check` passed

## Next Task

Start card
[`520-audit-rhai-host-surface-and-scaffold-lane.md`](./520-audit-rhai-host-surface-and-scaffold-lane.md).
