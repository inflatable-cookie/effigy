# 1034 - Add Validation Gap And Hotspot Scans

Roadmap: [`../006-validation-gap-and-hotspot-scans.md`](../006-validation-gap-and-hotspot-scans.md)
Strict lane: [`../../../specs/097-graph-aware-scan-intelligence-strict-lane.md`](../../../specs/097-graph-aware-scan-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-25

## Purpose

Identify high-impact graph nodes and changed areas with weak validation
signals.

## Work

- reuse `graph affected` and explore packet logic where possible
- identify central files or symbols with no nearby tests
- support changed-file validation mode
- emit likely tests separately from missing-test warnings
- add fixture proof for tested and untested hotspots

## Guardrails

- no claim that graph adjacency proves test coverage
- no release-gate dependency on noisy heuristic findings
- no invented test recommendations without graph evidence
- support repos with non-standard test naming without failing hard

## Acceptance

- fixture proves central tested and untested owners
- changed-file mode can use stdin or existing affected machinery
- JSON includes graph facts, likely tests, and confidence

## Next Task

Move to `1035`.
