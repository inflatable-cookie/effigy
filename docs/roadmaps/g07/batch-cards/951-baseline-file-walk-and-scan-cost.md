# 951 - Baseline File Walk And Scan Cost

Roadmap: [`../018-file-walk-and-scan-metadata-baseline.md`](../018-file-walk-and-scan-metadata-baseline.md)
Strict lane: [`../../../specs/087-graph-scan-cost-reduction-strict-lane.md`](../../../specs/087-graph-scan-cost-reduction-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Lock the real scan-cost baseline before changing graph walk behavior.

## Scope

- measure walk-only and scan-entry cost
- classify duplicated scan passes
- record the first profitable cut points

## Acceptance

- one baseline log drives `952` directly

## Results

- direct walk and metadata measurements show repo walking is now on the order
  of `40–50ms` per pass
- `graph index` still performs two walks in the no-op case
- `graph status` still performs four walks
- the duplicated scan passes are real, but the remaining win is now small
  enough that `952` is polish work, not rescue work

## Next Task

Execute `952`.
