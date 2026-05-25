# 1031 - Enrich Existing Scans With Graph Context

Roadmap: [`../003-existing-scan-graph-enrichment.md`](../003-existing-scan-graph-enrichment.md)
Strict lane: [`../../../specs/097-graph-aware-scan-intelligence-strict-lane.md`](../../../specs/097-graph-aware-scan-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-25

## Purpose

Add optional graph context to current scan findings without changing their base
filesystem meaning.

## Work

- pick the first two scan families with the best signal-to-risk ratio
- attach graph evidence such as inbound edges, outbound edges, centrality,
  likely owners, likely tests, or live-code references
- preserve original finding fields and severity
- add fixture coverage for graph-ready and graph-missing behavior

## Guardrails

- enrichment is additive
- graph-derived severity or priority changes must carry a reason
- no graph context may make a clean plain scan fail unless explicitly enabled
- avoid noisy fixture/generated/test false positives

## Acceptance

- at least two existing scan families produce useful optional graph context
- JSON and text output remain clear
- docs explain enrichment as context, not proof

## Next Task

Move to `1032`.
