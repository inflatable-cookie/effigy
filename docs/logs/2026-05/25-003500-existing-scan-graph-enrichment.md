# Existing Scan Graph Enrichment

Date: 2026-05-25
Roadmap: [`g08.003`](../roadmaps/g08/003-existing-scan-graph-enrichment.md)
Strict lane: [`097`](../specs/097-graph-aware-scan-intelligence-strict-lane.md)
Batch card: [`1031`](../roadmaps/g08/batch-cards/1031-enrich-existing-scans-with-graph-context.md)

## What Landed

`effigy scan --graph-context` now does real additive enrichment for the first
two scan families:

- `god-files`
- `attention-markers`

When graph context is requested and a usable index exists, matching findings
now carry file-level graph facts:

- `language_id`
- `symbol_count`
- `inbound_edges`
- `outbound_edges`
- `reference_count`
- `connectivity`

The enrichment is file-path based and bounded. It does not change scan
severity, thresholds, or plain filesystem behavior.

## Chosen Shape

The implementation stays in the built-in scan orchestration layer.

The scan engine still owns:

- file traversal
- matching
- thresholds
- finding order

The built-in layer now:

- loads graph readiness when `--graph-context` is requested
- loads graph file facts only for scan families that explicitly support
  enrichment
- attaches facts to matching findings by relative path
- marks `graph.applied = true` only when at least one finding was enriched

Unsupported scan families keep the `1030` behavior:

- readiness is reported
- enrichment remains unapplied
- no fake “matched nothing” message replaces the “not implemented yet” reason

## Output Shape

JSON:

- existing schema names stay `effigy.scan.*.v1`
- enriched findings now add an optional `graph` object
- the top-level additive `graph` block from `1030` remains unchanged

Text and markdown:

- enriched finding rows add a `graph:` line in text mode
- markdown reports add a `Graph` column only when any finding has graph facts
- the top-level graph note now says `applied` when enrichment really happened

## Proof

Unit proof:

- `graph_facts_enrich_matching_findings_by_path`

Runner proof:

- `run_manifest_task_builtin_scan_god_files_graph_context_enriches_findings`
- `run_manifest_task_builtin_scan_attention_markers_graph_context_enriches_findings`

JSON contract proof:

- `builtin_scan_god_files_graph_context_json_contract_enriches_findings`
- `builtin_scan_attention_markers_graph_context_json_contract_enriches_findings`

## Residual Limits

- enrichment is still file-level, not symbol-level
- no graph-derived severity changes exist yet
- no graph-native scan families exist yet
- unsupported scan families still report readiness only

## Vision Target Delta

- tags: `ROUTE`, `MAINT`
- moved:
  - `scan --graph-context` went from readiness-only to real additive
    enrichment for `god-files` and `attention-markers`
  - scan findings can now carry concrete graph evidence without changing their
    base contract
- remains open:
  - boundary/layer scans
  - dead/isolated code scans
  - validation-gap and hotspot scans
