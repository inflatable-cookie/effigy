# 988 - Expand Language Extractor Priority Set

Roadmap: [`../039-richer-language-extractor-coverage.md`](../039-richer-language-extractor-coverage.md)
Strict lane: [`../../../specs/091-codegraph-parity-strict-lane.md`](../../../specs/091-codegraph-parity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Add high-value first-party language extractors without changing Effigy's Rust
binary and no-plugin posture.

## Work

- choose the first concrete language slice from the priority order
- evaluate parser dependencies, licensing, build impact, and failure behavior
- implement extractor facts, edges, diagnostics, and tests
- add mixed-language fixtures where the language appears in real agent flows
- update docs with supported and unsupported language claims
- measure index/runtime impact

## Acceptance

- at least one high-priority language extractor lands with fixture coverage
- dependency impact is recorded
- failures are diagnostic-only
- benchmark cross-language tasks improve or the next language priority is
  re-scoped with evidence

## Evidence

- [`2026-05/18-154729-python-extractor-slice.md`](../../../logs/archive/2026-05/18-154729-python-extractor-slice.md)

## Next Task

Execute `989`.
