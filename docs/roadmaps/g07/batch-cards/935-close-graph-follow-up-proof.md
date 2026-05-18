# 935 - Close Graph Follow-Up Proof

Roadmap: [`../013-graph-follow-up-performance-and-fixture-reliability.md`](../013-graph-follow-up-performance-and-fixture-reliability.md)
Strict lane: [`../../../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md`](../../../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Close the follow-up graph lane with refreshed measurements and explicit
residual limits.

## Scope

- rerun the measured proof
- compare against the `g07.012` baseline
- record wins, misses, and retained limits
- update front doors

## Acceptance

- closeout log records before/after results
- any retained gap is explicit, not implied

## Results

- refreshed the lane-wide proof against the `g07.012` baseline
- confirmed:
  - no-op indexing is materially cheaper
  - key graph query surfaces are faster
  - full-repo `failed_paths` is now empty
- recorded residual limits in the closeout log instead of leaving them implied
- closed `g07.013` and strict lane `086`

## Next Task

Open the next graph tranche only if warning-level template compose depth,
lexical-query competitiveness, or file-walk cost becomes worth the extra lane.
