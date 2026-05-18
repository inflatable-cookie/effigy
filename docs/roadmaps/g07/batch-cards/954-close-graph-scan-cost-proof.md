# 954 - Close Graph Scan Cost Proof

Roadmap: [`../017-graph-scan-cost-reduction-suite.md`](../017-graph-scan-cost-reduction-suite.md)
Strict lane: [`../../../specs/087-graph-scan-cost-reduction-strict-lane.md`](../../../specs/087-graph-scan-cost-reduction-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Close the scan-cost lane with measured before/after proof and residual limits.

## Scope

- rerun the no-op index proof
- rerun status proof
- compare with `g07.013`
- refresh front doors

## Acceptance

- closeout log records the final delta clearly

## Results

- refreshed the final scan-cost proof against the `g07.013` baseline
- confirmed the lane delivered real bounded wins:
  - `graph index --json` no-op path now sits around `0.25s`
  - `graph status --json` now sits around `0.21s` to `0.24s`
- closed `g07.017` and strict lane `087`

## Next Task

Leave the graph surface parked unless a future lane justifies riskier cache or
watcher design.
