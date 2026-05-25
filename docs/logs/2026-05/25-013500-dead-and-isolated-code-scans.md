# Dead And Isolated Code Scans

Date: 2026-05-25
Roadmap: [`g08.005`](../roadmaps/g08/005-dead-and-isolated-code-scans.md)
Strict lane: [`097`](../specs/097-graph-aware-scan-intelligence-strict-lane.md)
Batch card: [`1033`](../roadmaps/g08/batch-cards/1033-add-dead-and-isolated-code-scans.md)

## What Landed

`effigy scan dead-code` now exists as the second graph-native scan family.

It reports advisory dead-code candidates backed by concrete graph evidence:

- `isolated-file`
- `unreferenced-symbol`

The scan only inspects implementation-shaped files in languages whose graph
extractors advertise symbol plus relation coverage. Tests, docs, fixtures,
generated files, config, migrations, and entrypoint-like scripts stay out by
default.

## Chosen Shape

The manifest contract is optional and allowlist-driven:

```toml
[scan.dead_code]
doctor = false
allow_paths = ["src/bin/**", "scripts/**"]
allow_symbols = ["crate::bootstrap::*", "main"]
```

Current rules:

- graph index must be usable
- heuristic edges and references stay excluded unless the config opts in
- crate roots and entrypoint-like script paths are excluded by role
- intentional isolated code can be suppressed with path or symbol globs
- findings stay advisory with explicit `confidence` and `reason` fields

## Output Shape

The new scan family uses:

- schema: `effigy.scan.dead-code.v1`
- result fields:
  - `checked_files`
  - `checked_symbols`
  - `skipped_allowlisted_paths`
  - `skipped_allowlisted_symbols`
  - `skipped_non_implementation_files`
  - `skipped_unsupported_language_files`
  - `findings`

Each finding includes:

- `kind`
- `path`
- `line`
- `symbol`
- `symbol_kind`
- `language_id`
- `confidence`
- `severity`
- `reason`
- `inbound_edges`
- `outbound_edges`
- `inbound_references`
- `outbound_references`

## Proof

Manifest/config proof:

- `scan_config_accepts_dead_code_allowlists`
- config schema target includes the new scan section

Runner proof:

- `run_manifest_task_builtin_scan_dead_code_reports_isolated_and_unreferenced_findings`
- `run_manifest_task_builtin_scan_dead_code_respects_symbol_allowlist`

JSON proof:

- `builtin_scan_dead_code_json_contract_reports_advisory_findings`

Docs proof:

- guide example and safe-review note added to `076`
- link checks passed

## Residual Limits

- current findings are candidate-level only; there is no compiler-grade proof
- language support is gated by extractor capability, not a universal guarantee
- public-surface and orphaned-entrypoint candidate findings remain for later
  follow-through if they can be added without noise

## Vision Target Delta

- tags: `ROUTE`, `CONTRACT`, `MAINT`
- moved:
  - graph-native scans now cover architecture boundaries plus likely dead-code
    candidates
  - repos can allowlist intentional bootstrap and entrypoint surfaces without
    turning the scan off
- remains open:
  - validation-gap and hotspot scans
  - broader graph-aware scan docs and benchmark proof
