# 1018 - Trim High Help Topic Duplicates

Roadmap: [`../068-high-duplicate-help-fragment-reduction.md`](../068-high-duplicate-help-fragment-reduction.md)
Strict lane: [`../../../specs/095-residual-maintainability-follow-through-strict-lane.md`](../../../specs/095-residual-maintainability-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Reduce the remaining high duplicate help-topic fragments without making the
topics harder to read.

## Work

- target the current high duplicate clusters in bootstrap, demo, container, and
  release help topics
- extract only obvious shared fragments
- pin touched topic output with focused tests

## Guardrails

- no mega-help abstraction
- no readability loss for scan-score vanity
- no accidental CLI wording churn

## Acceptance

- high duplicate help-topic findings are reduced or eliminated
- focused help tests pass

## Next Task

Execute `1019`.
