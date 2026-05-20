# 1020 - Converge Runner Private Fixtures And Helpers

Roadmap: [`../070-runner-private-fixture-and-helper-convergence.md`](../070-runner-private-fixture-and-helper-convergence.md)
Strict lane: [`../../../specs/095-residual-maintainability-follow-through-strict-lane.md`](../../../specs/095-residual-maintainability-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Reduce the remaining runner-private duplication clusters where reuse is real
and ownership is already obvious.

## Work

- target the current high temp-repo duplicate first
- inspect local vault/test-secret setup duplication if time remains in the same
  bounded batch
- keep helpers close to the owner module or test family
- run focused runner validation

## Guardrails

- no public helper API for internal convenience
- no fixture abstraction that hides scenario meaning
- no broad helper tour across the repo

## Acceptance

- the current high runner-private duplicate is removed or clearly justified
- focused runner tests pass

## Next Task

Execute `1021`.
