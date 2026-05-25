# 1035 - Update Agent Docs, JSON, And Benchmark Proof

Roadmap: [`../007-agent-docs-json-and-benchmark-proof.md`](../007-agent-docs-json-and-benchmark-proof.md)
Strict lane: [`../../../specs/097-graph-aware-scan-intelligence-strict-lane.md`](../../../specs/097-graph-aware-scan-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-25

## Purpose

Make graph-aware scans usable by agents without turning them into another
startup ritual.

## Work

- update scan docs and command reference
- add JSON examples for graph-enriched and graph-required outputs
- update Effigy skill guidance by job
- add fixture-backed proof or benchmark task
- include optional live-repo proof with skip behavior when useful

## Guardrails

- do not over-promote graph-aware scans above other Effigy surfaces
- do not claim speed or tool-call reductions without evidence
- do not require private local repos for validation
- stale or missing graph states must remain visible

## Acceptance

- docs and skill route agents clearly
- JSON examples pass docs checks
- proof task is deterministic for fixtures

## Next Task

Move to `1036`.
