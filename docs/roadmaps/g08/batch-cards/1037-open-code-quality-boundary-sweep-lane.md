# 1037 - Open Code Quality Boundary Sweep Lane

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: planning-only until a follow-up spec is explicitly opened.

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Record the code-quality sweep baseline and prepare the first implementation
card without changing behavior.

## Work

- capture the 2026-06-04 sweep findings in a short evidence log
- inventory command-surface declaration points
- inventory Rhai feature declaration and dispatch points
- record the current scan outputs and doctor caveat
- decide whether the first implementation card needs a strict-lane spec
  before implementation

## Guardrails

- no code changes
- no command behavior changes
- no release mutation
- no `.github/workflows/` edits
- do not mark downstream cards ready until their owner and acceptance criteria
  are concrete

## Acceptance

- evidence log exists
- the sweep has enough context for a bounded implementation card
- existing doctor fixture-schema error is recorded, not silently folded into
  this lane

## Evidence

- [`../../../logs/2026-06/04-204300-code-quality-boundary-sweep-lane-opened.md`](../../../logs/2026-06/04-204300-code-quality-boundary-sweep-lane-opened.md)

## Validation

- `effigy tasks`
- `effigy doctor`
- `effigy scan duplicate-blocks --json`
- `effigy scan boundary-violations --json`
- `effigy test --plan`

## Next Task

Run `1038`.
