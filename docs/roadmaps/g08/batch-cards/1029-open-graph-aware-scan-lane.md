# 1029 - Open Graph-Aware Scan Lane

Roadmap: [`../001-graph-aware-scan-intelligence-suite.md`](../001-graph-aware-scan-intelligence-suite.md)
Strict lane: [`../../../specs/097-graph-aware-scan-intelligence-strict-lane.md`](../../../specs/097-graph-aware-scan-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-25

## Purpose

Lock the baseline before scan and graph contracts are connected.

## Work

- inspect current scan command families and JSON output
- inspect current graph readiness/status payloads
- record which scans are filesystem-only and which could accept enrichment
- record candidate fixture repos for graph-backed scan proof
- write an opening log with strict non-goals and acceptance criteria

## Guardrails

- no code changes in this card unless needed to collect baseline evidence
- no command contract changes
- no graph indexing side effects from scan
- no Effigy-only scan rule design

## Acceptance

- opening baseline log exists
- scan families and graph readiness surfaces are inventoried
- `1030` has enough evidence to define the contract

## Next Task

Move to `1030`.
