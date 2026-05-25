# g08 Roadmaps

Status: Complete
Theme: Graph-aware scan intelligence

## Purpose

`g08` connects Effigy's scan surface to the code graph without making the
existing scans slower, fuzzy, or index-dependent.

The goal is not to replace deterministic filesystem scans. The goal is to add
relation-aware findings where graph data is the missing signal, and to enrich
current scan output when a ready index already exists.

This generation should help agents and maintainers answer questions like:

- which oversized files are also high-blast-radius owners?
- which TODOs sit on central or boundary-crossing code?
- which symbols or files look unused, isolated, or under-tested?
- which imports/calls violate declared architecture boundaries?
- which changed areas need validation attention before merge?

## Roadmap Sequence

- [`001-graph-aware-scan-intelligence-suite.md`](./001-graph-aware-scan-intelligence-suite.md)
- [`002-scan-graph-contract-and-readiness-model.md`](./002-scan-graph-contract-and-readiness-model.md)
- [`003-existing-scan-graph-enrichment.md`](./003-existing-scan-graph-enrichment.md)
- [`004-boundary-and-layer-violation-scans.md`](./004-boundary-and-layer-violation-scans.md)
- [`005-dead-and-isolated-code-scans.md`](./005-dead-and-isolated-code-scans.md)
- [`006-validation-gap-and-hotspot-scans.md`](./006-validation-gap-and-hotspot-scans.md)
- [`007-agent-docs-json-and-benchmark-proof.md`](./007-agent-docs-json-and-benchmark-proof.md)
- [`008-graph-aware-scan-closeout.md`](./008-graph-aware-scan-closeout.md)

## Design Posture

- keep existing scan commands deterministic and useful without a graph index
- make graph-backed behavior explicit in JSON and human output
- use graph data for relationships, not vague scoring
- preserve source paths, ranges, and reasons for every finding
- prefer repo-agnostic rules, fixtures, and configuration over Effigy-specific
  assumptions
- keep exact-token proof and final code inspection outside graph claims

## Non-Goals

- no hidden auto-indexing from `scan`
- no LLM-generated scan findings
- no MCP server or graph daemon
- no Effigy-only architecture rule hard-coding
- no breaking rewrite of existing `scan` JSON contracts
- no claim that graph-backed scans prove semantic dead code with compiler
  precision

## Execution Rule

Open a strict lane and batch cards only when implementation starts.

The first execution batch should record the current scan and graph command
contracts, then add the smallest additive graph-readiness contract needed for
later scan work.

## Batch Cards

- [`1029-open-graph-aware-scan-lane.md`](./batch-cards/1029-open-graph-aware-scan-lane.md)
- [`1030-define-scan-graph-readiness-contract.md`](./batch-cards/1030-define-scan-graph-readiness-contract.md)
- [`1031-enrich-existing-scans-with-graph-context.md`](./batch-cards/1031-enrich-existing-scans-with-graph-context.md)
- [`1032-add-boundary-and-layer-violation-scans.md`](./batch-cards/1032-add-boundary-and-layer-violation-scans.md)
- [`1033-add-dead-and-isolated-code-scans.md`](./batch-cards/1033-add-dead-and-isolated-code-scans.md)
- [`1034-add-validation-gap-and-hotspot-scans.md`](./batch-cards/1034-add-validation-gap-and-hotspot-scans.md)
- [`1035-update-agent-docs-json-and-benchmark-proof.md`](./batch-cards/1035-update-agent-docs-json-and-benchmark-proof.md)
- [`1036-close-graph-aware-scan-lane.md`](./batch-cards/1036-close-graph-aware-scan-lane.md)

## Current State

`g07` is closed through `g07.078`.

`g08` is closed through `g08.008`.

No active ready card remains.

## Next Task

Planning only.
