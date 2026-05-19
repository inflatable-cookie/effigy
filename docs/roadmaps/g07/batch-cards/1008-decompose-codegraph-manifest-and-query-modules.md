# 1008 - Decompose Codegraph Manifest And Query Modules

Roadmap: [`../058-codegraph-manifest-query-module-decomposition.md`](../058-codegraph-manifest-query-module-decomposition.md)
Strict lane: [`../../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md`](../../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md)

Status: Planned
Owner: Platform
Created: 2026-05-19

## Purpose

Split the oversized graph manifest and query modules into readable ownership
units.

## Work

- split manifest extraction by manifest section or graph fact family
- split query code by ranking, source evidence, traversal, packet assembly, and
  response shaping
- keep facades stable for current callers
- move tests only when it improves failure locality
- rerun graph tests and god-file scan

## Guardrails

- no storage migration
- no public JSON contract drift
- no ranking rewrite
- no broad fixture churn

## Acceptance

- `manifest.rs` and `query.rs` are materially smaller or any remaining size is
  justified
- graph tests pass
- CLI output contracts remain stable

## Next Task

Start [`1009-split-init-setup-inventory-and-wizard-boundaries.md`](./1009-split-init-setup-inventory-and-wizard-boundaries.md).
