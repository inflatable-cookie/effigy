# 1033 - Add Dead And Isolated Code Scans

Roadmap: [`../005-dead-and-isolated-code-scans.md`](../005-dead-and-isolated-code-scans.md)
Strict lane: [`../../../specs/097-graph-aware-scan-intelligence-strict-lane.md`](../../../specs/097-graph-aware-scan-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-25

## Purpose

Find likely unused, isolated, or orphaned code using graph connectivity.

## Work

- define advisory finding types and confidence levels
- distinguish implementation code from tests, docs, generated files, scripts,
  migrations, fixtures, and intentional entrypoints
- add suppression or allowlist behavior for intentional isolation
- report only findings backed by concrete graph evidence

## Guardrails

- findings must say likely or candidate, not proven dead code
- no low-confidence failures by default
- no claims for unsupported languages without explicit graph coverage
- no delete guidance without human review

## Acceptance

- fixture proves isolated code and intentional-entrypoint exclusion
- output includes confidence and reason fields
- docs describe safe review workflow

## Next Task

Move to `1034`.
