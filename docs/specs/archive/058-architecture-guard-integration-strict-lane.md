# 058 - Architecture Guard Integration Strict Lane

Roadmap: [`g04.016`](../roadmaps/g04/016-architecture-guard-integration.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Purpose

Make runtime/container architecture guards part of normal validation instead of
an optional side task.

## Hard Boundaries

- no release work
- no `.github/workflows/` edits
- keep guard output fast and grep-readable
- keep allowlisted debt explicit and path-scoped

## Current Ready Card

[`586-wire-architecture-guard-into-validation-aggregators.md`](./batch-cards/586-wire-architecture-guard-into-validation-aggregators.md)

## Execution Chain

- `586` ready: wire `qa:architecture` into normal validation aggregators and
  document suppression policy

## Exit Condition

This lane closes when normal validation catches runtime/container architecture
drift and the suppression policy is documented.

## Next Task

Card
[`587-split-effigy-container-ops-module-owners.md`](./batch-cards/587-split-effigy-container-ops-module-owners.md).
