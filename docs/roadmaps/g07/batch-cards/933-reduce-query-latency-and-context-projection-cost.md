# 933 - Reduce Query Latency And Context Projection Cost

Roadmap: [`../015-query-speed-and-projection-reduction.md`](../015-query-speed-and-projection-reduction.md)
Strict lane: [`../../../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md`](../../../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Reduce the obvious graph query hot-path cost without weakening the query
contracts.

## Scope

- trim whole-store reload patterns
- reduce over-fetching for `status`, `search`, and `context`
- add focused latency proof after the changes

## Acceptance

- measured query latency improves from the `g07.012` baseline
- JSON contracts and result determinism stay stable

## Next Task

Execute `934`.
