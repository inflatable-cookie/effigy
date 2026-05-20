# 1011 - Trim Runner Domain And Test Fixture Duplication

Roadmap: [`../061-runner-domain-boundary-and-test-fixture-cleanup.md`](../061-runner-domain-boundary-and-test-fixture-cleanup.md)
Strict lane: [`../../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md`](../../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Reduce older runner/test duplication where the boundary is clear and the risk is
low.

## Work

- choose one high-noise fixture pattern and replace it with a local builder
- inspect one runner command surface for pure planner/domain extraction
- keep shell/process glue in runner
- avoid cross-crate fixture support until actual reuse proves it
- run focused tests for touched surfaces

## Guardrails

- no runner rewrite
- no behavior drift in task execution
- no release or container safety weakening
- no abstract test helper that hides scenario intent

## Acceptance

- at least one fixture duplication pattern is cleaner
- any runner extraction has an obvious owner
- touched tests remain readable

## Next Task

Start [`1012-review-crate-boundaries-and-planning-hygiene.md`](./1012-review-crate-boundaries-and-planning-hygiene.md).
