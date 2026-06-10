# Boundary And Layer Violation Scans

Date: 2026-05-25
Roadmap: [`g08.004`](../roadmaps/g08/004-boundary-and-layer-violation-scans.md)
Strict lane: [`097`](../specs/097-graph-aware-scan-intelligence-strict-lane.md)
Batch card: [`1032`](../roadmaps/g08/batch-cards/1032-add-boundary-and-layer-violation-scans.md)

## What Landed

`effigy scan boundary-violations` now exists as the first graph-native scan
family.

It checks configured path layers against graph edges and reports disallowed
cross-layer dependencies with concrete source and target evidence.

## Chosen Shape

The manifest contract is optional and path-based:

```toml
[scan.boundary_violations]
doctor = false

[scan.boundary_violations.layers.app]
paths = ["src/app/**"]
may_depend_on = ["domain", "shared"]
```

Current rules:

- no configured layers:
  - clean no-rules result
- configured layers:
  - resolved edges are checked directly
  - unresolved syntactic imports are resolved locally against indexed symbols
    when the match is unique
  - heuristic edges stay excluded unless the config opts in
  - ambiguous layer matches fail as config errors instead of being guessed

## Output Shape

The new scan family uses:

- schema: `effigy.scan.boundary-violations.v1`
- result fields:
  - `configured_layers`
  - `checked_edges`
  - `findings`

Each finding includes:

- `source_layer`
- `target_layer`
- `edge_kind`
- `source_path`
- `source_line`
- `source_symbol`
- `target_path`
- `target_line`
- `target_symbol`
- `confidence`
- `severity`

## Proof

Manifest/config proof:

- `scan_config_accepts_boundary_violation_layers`
- config schema target includes the new scan section

Runner proof:

- `run_manifest_task_builtin_scan_boundary_violations_reports_disallowed_edges`
- `run_manifest_task_builtin_scan_boundary_violations_without_rules_is_clean`

JSON proof:

- `builtin_scan_boundary_violations_json_contract_reports_precise_findings`

Docs proof:

- guide example added to `076`
- link checks passed

## Residual Limits

- only path layers are supported in this batch
- no symbol-group rules yet
- no task-dependency or manifest-edge boundary checks yet
- unresolved imports only resolve when symbol matching is unique

## Vision Target Delta

- tags: `ROUTE`, `CONTRACT`, `MAINT`
- moved:
  - graph-aware scan work now includes the first graph-native scan family
  - repos can declare simple path layers and catch disallowed cross-layer
    imports/dependencies without Effigy-specific hard-coding
- remains open:
  - dead/isolated code scans
  - validation-gap and hotspot scans
  - broader graph-aware scan docs and benchmark proof
