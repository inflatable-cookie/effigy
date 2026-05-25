# 1030 - Define Scan Graph Readiness Contract

Roadmap: [`../002-scan-graph-contract-and-readiness-model.md`](../002-scan-graph-contract-and-readiness-model.md)
Strict lane: [`../../../specs/097-graph-aware-scan-intelligence-strict-lane.md`](../../../specs/097-graph-aware-scan-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-25

## Purpose

Give scan commands a stable, explicit way to report whether graph data was
used, skipped, missing, stale, or degraded.

## Work

- choose the command shape for graph-required versus graph-enriched scans
- add the smallest shared readiness payload needed by scan output
- pin ready, missing, stale, and degraded states in tests
- document how scan behaves when graph data is unavailable

## Guardrails

- no hidden `graph index`
- no DB-layout leakage in JSON
- no breaking changes to existing scan output
- no broad scan implementation rewrite

## Acceptance

- JSON readiness fields are test-backed
- text output is clear when graph data is skipped
- existing scans remain usable without graph data

## Next Task

Move to `1031`.
