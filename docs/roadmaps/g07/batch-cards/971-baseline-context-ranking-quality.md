# 971 - Baseline Context Ranking Quality

Roadmap: [`../025-graph-context-ranking-quality-suite.md`](../025-graph-context-ranking-quality-suite.md)
Strict lane: [`../../../specs/089-graph-navigation-ranking-quality-strict-lane.md`](../../../specs/089-graph-navigation-ranking-quality-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Define and capture gold-task ranking evidence before changing the context
ranker.

## Scope

- select 5 to 8 representative agent navigation tasks
- run `graph context`, `graph search`, and direct `rg` where useful
- record top results, timings, and failure classifications
- add or prepare regression tests for rank direction

## Acceptance

- baseline log exists:
  [`18-173500-graph-context-ranking-baseline.md`](../../../logs/2026-05/18-173500-graph-context-ranking-baseline.md)
- expected top-file sets are explicit
- implementation tasks, docs tasks, and test-intent tasks are covered
- `972` has a concrete failure set to close

## Next Task

Execute `972`.
