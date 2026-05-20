# 1019 - Follow Through Language Emitter Duplicates

Roadmap: [`../069-language-emitter-follow-through.md`](../069-language-emitter-follow-through.md)
Strict lane: [`../../../specs/095-residual-maintainability-follow-through-strict-lane.md`](../../../specs/095-residual-maintainability-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Inspect the remaining high duplicate blocks across JS, PHP, and Python emitters
and reduce only the duplication that still represents real maintenance risk.

## Work

- classify each remaining high emitter duplicate as:
  - profitable shared helper
  - acceptable local duplication
  - not worth chasing
- extract only helpers that keep the call sites readable
- run focused extractor/codegraph validation

## Guardrails

- no generic extraction framework
- no helper that obscures provenance, IDs, or traversal meaning
- no optimization-only side work

## Acceptance

- each high emitter duplicate is removed or explicitly justified
- focused codegraph tests pass

## Next Task

Execute `1020`.
