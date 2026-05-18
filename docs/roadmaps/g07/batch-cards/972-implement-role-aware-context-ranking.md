# 972 - Implement Role-Aware Context Ranking

Roadmap: [`../025-graph-context-ranking-quality-suite.md`](../025-graph-context-ranking-quality-suite.md)
Strict lane: [`../../../specs/089-graph-navigation-ranking-quality-strict-lane.md`](../../../specs/089-graph-navigation-ranking-quality-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Improve context ranking so implementation questions prefer implementation files
without hiding tests, docs, examples, or fixtures when they are requested.

## Scope

- add generic file-role classification
- add request-intent classification
- normalize and de-noise query tokens
- cap repeated symbol-hit inflation
- prefer phrase and multi-token co-occurrence
- preserve explainable reasons

## Acceptance

- gold implementation tasks rank implementation files first
- docs/test intent changes rank direction as expected
- scoring remains deterministic
- focused tests cover role behavior and repeated-symbol caps
- evidence log exists:
  [`18-181500-role-aware-graph-context-ranking.md`](../../../logs/2026-05/18-181500-role-aware-graph-context-ranking.md)

## Next Task

Execute `973`.
