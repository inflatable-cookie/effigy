# 994 - Run CodeGraph Parity Closeout

Roadmap: [`../045-codegraph-parity-closeout.md`](../045-codegraph-parity-closeout.md)
Strict lane: [`../../../specs/091-codegraph-parity-strict-lane.md`](../../../specs/091-codegraph-parity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Run the full parity benchmark and write down the honest result.

## Work

- rerun the `985` harness
- compare tool calls, file reads, time, bytes, and cold/warm posture
- classify each gap as closed, accepted, deferred, or non-goal
- update docs to avoid unsupported claims
- decide whether any residual graph work stays in `g07`

## Acceptance

- closeout log exists
- conclusions are evidence-backed
- no known weak query is hidden
- `995` has the correct close/rescope target

## Evidence

- [`2026-05/18-174500-codegraph-parity-closeout.md`](../../../logs/archive/2026-05/18-174500-codegraph-parity-closeout.md)

## Next Task

No active ready card. Closeout is complete; any follow-up work needs a new
bounded planning lane.
