# 990 - Harden Source Section No-Reread Packets

Roadmap: [`../041-source-section-packets-and-no-reread-workflow.md`](../041-source-section-packets-and-no-reread-workflow.md)
Strict lane: [`../../../specs/091-codegraph-parity-strict-lane.md`](../../../specs/091-codegraph-parity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Make `graph explore` return source sections that are useful enough to avoid
immediate rereads for normal first-pass agent navigation.

## Work

- define section boundaries for implemented languages and docs/manifests
- add completeness/truncation metadata
- tune byte allocation by role and relation kind
- improve overflow guidance
- add JSON examples for complete, truncated, and omitted sections
- rerun zero-reread benchmark cases

## Acceptance

- returned sections support first-pass reasoning without opening the same file
- truncation is explicit
- payload sizes stay bounded
- docs teach the no-reread rule without overclaiming edit readiness

## Evidence

- [`2026-05/18-160609-source-section-packets.md`](../../../logs/archive/2026-05/18-160609-source-section-packets.md)

## Next Task

Execute `991`.
