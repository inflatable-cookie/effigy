# 1032 - Add Boundary And Layer Violation Scans

Roadmap: [`../004-boundary-and-layer-violation-scans.md`](../004-boundary-and-layer-violation-scans.md)
Strict lane: [`../../../specs/097-graph-aware-scan-intelligence-strict-lane.md`](../../../specs/097-graph-aware-scan-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-25

## Purpose

Use graph edges to find configured architecture boundary violations.

## Work

- design a small manifest rule shape for boundary groups
- support path-based rules first
- detect disallowed graph edges between groups
- emit source group, target group, edge kind, path, and range evidence
- add no-config behavior that exits cleanly

## Guardrails

- no hard-coded Effigy crate names
- no mandatory boundary config
- no noisy test-only or self-edge findings unless configured
- heuristic edges must be clearly marked or excluded by default

## Acceptance

- fixture proves allowed and rejected edges
- JSON finding evidence is precise
- docs include one small generic config example

## Next Task

Move to `1033`.
