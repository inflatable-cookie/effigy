# 1016 - Decompose Codegraph Test Harness

Roadmap: [`../066-codegraph-test-harness-decomposition.md`](../066-codegraph-test-harness-decomposition.md)
Strict lane: [`../../../specs/095-residual-maintainability-follow-through-strict-lane.md`](../../../specs/095-residual-maintainability-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Split the codegraph crate’s mixed proof surface into clearer test owners.

## Work

- separate graph tests by behavior family
- keep local graph test support readable and nearby
- preserve proof depth and fixture realism
- run focused codegraph tests after the move

## Guardrails

- no test-coverage reduction for line-count optics
- no opaque helper stack that hides scenario setup
- no unrelated crate movement

## Acceptance

- `tests.rs` disappears or becomes a thin facade
- failure locality is clearer
- focused graph tests pass

## Next Task

Execute `1017`.
