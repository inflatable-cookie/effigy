# g08 Roadmaps

Status: Active
Theme: Graph-aware scan intelligence and code quality boundary follow-through,
extended with the 2026-06-10 security and posture hardening tranche
(g08.010–g08.015, complete). Generation remains open for further scope.

## Purpose

`g08` connects Effigy's scan surface to the code graph without making the
existing scans slower, fuzzy, or index-dependent. The follow-up tranche uses
that scan evidence plus a manual code-quality sweep to reduce drift-prone
declarations and mixed ownership boundaries.

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
- [`009-code-quality-boundary-sweep-suite.md`](./009-code-quality-boundary-sweep-suite.md)
- [`010-security-and-posture-hardening-suite.md`](./010-security-and-posture-hardening-suite.md)
- [`011-discovery-and-doctor-correctness.md`](./011-discovery-and-doctor-correctness.md)
- [`012-supply-chain-and-ci-security-gates.md`](./012-supply-chain-and-ci-security-gates.md)
- [`013-daemon-panic-safety-and-secret-egress-hardening.md`](./013-daemon-panic-safety-and-secret-egress-hardening.md)
- [`014-gateway-route-table-trust-model.md`](./014-gateway-route-table-trust-model.md)
- [`015-docs-spine-compaction.md`](./015-docs-spine-compaction.md)
- [`016-suppression-hygiene-and-dead-code-precision.md`](./016-suppression-hygiene-and-dead-code-precision.md)
- [`017-workspace-ssh-agent-mount-resilience.md`](./017-workspace-ssh-agent-mount-resilience.md)

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
- no public command behavior changes while reducing declaration drift
- no speculative macro framework or generated command surface
- no broad code deletion from advisory scan findings

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
- [`1037-open-code-quality-boundary-sweep-lane.md`](./batch-cards/1037-open-code-quality-boundary-sweep-lane.md)
- [`1038-define-command-surface-descriptor-seam.md`](./batch-cards/1038-define-command-surface-descriptor-seam.md)
- [`1039-define-rhai-feature-descriptor-seam.md`](./batch-cards/1039-define-rhai-feature-descriptor-seam.md)
- [`1040-split-container-up-phase-helpers.md`](./batch-cards/1040-split-container-up-phase-helpers.md)
- [`1041-converge-repo-marker-rules.md`](./batch-cards/1041-converge-repo-marker-rules.md)
- [`1042-reduce-selected-duplicate-blocks.md`](./batch-cards/1042-reduce-selected-duplicate-blocks.md)
- [`1043-tune-boundary-and-dead-code-scans-for-effigy.md`](./batch-cards/1043-tune-boundary-and-dead-code-scans-for-effigy.md)
- [`1044-fix-dead-code-scan-rust-signal.md`](./batch-cards/1044-fix-dead-code-scan-rust-signal.md)
- [`1045-classify-and-reduce-dead-code-residuals.md`](./batch-cards/1045-classify-and-reduce-dead-code-residuals.md)
- [`1046-classify-trait-and-api-surface-dead-code.md`](./batch-cards/1046-classify-trait-and-api-surface-dead-code.md)
- [`1047-classify-descriptor-and-dispatch-dead-code-roots.md`](./batch-cards/1047-classify-descriptor-and-dispatch-dead-code-roots.md)
- [`1048-classify-dto-render-config-dead-code-roots.md`](./batch-cards/1048-classify-dto-render-config-dead-code-roots.md)
- [`1049-classify-rust-impl-and-associated-call-dead-code.md`](./batch-cards/1049-classify-rust-impl-and-associated-call-dead-code.md)
- [`1050-complete-dead-code-false-positive-burn-down.md`](./batch-cards/1050-complete-dead-code-false-positive-burn-down.md)

## Current State

`g07` is closed through `g07.078`.

`g08` is complete through `g08.009`.

No active ready card.

## Next Task

No current dead-code residual batch remains.
