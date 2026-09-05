# Dead-Code Scan Rust Signal Correction

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1044`

## Summary

Fixed the dead-code scan weakness found during Effigy self-adoption.

The previous repo-level `allow_symbols = ["*"]` workaround is removed. The scan
now reduces noise through graph/indexer behavior.

## Changes

- Dead-code analysis now resolves uniquely matched unresolved edge and
  reference targets.
- Rust module declarations now prevent matching module files from being
  reported as isolated.
- Rust public API roots are skipped as private dead-code symbol candidates.
- Rust call-site graph facts now emit syntactic confidence instead of heuristic
  confidence.
- The Rust graph extractor version moved from `0.1.0` to `0.1.1` so existing
  graph indexes refresh the changed call-site facts.
- Boundary scan target resolution now uses a precomputed symbol lookup, avoiding
  an O(edges x symbols) path after more Rust call edges became eligible.
- Effigy's `scan.dead_code.allow_symbols = ["*"]` workaround was removed.

## Result

After a graph refresh, `target/debug/effigy scan dead-code --json` reports:

- checked files: 798
- checked symbols: 3,177
- findings: 1,178
- isolated files: 5
- unreferenced symbols: 1,173

The previous unsuppressed self-scan reported 6,497 findings. This pass removes
the broad false-positive class without hiding all symbol findings.

`target/debug/effigy scan boundary-violations --json` reports:

- configured layers: 2
- checked edges: 34
- findings: 0

## Remaining Queue

The remaining findings are still advisory. The largest groups are in
impl-heavy and dispatch-heavy Rust modules where graph precision can improve
further:

- command dispatch and rendering helpers
- manifest composition/config-section helpers
- container lifecycle/data helpers
- runtime data planning/volume helpers

Next likely scan improvements:

- better method/associated-function ownership and call matching
- trait method and impl method public-root handling
- path-qualified Rust call normalization beyond simple unique-name matching

## Validation

- `cargo test -p effigy-codegraph graph_rust_indexer_emits_module_import_and_syntactic_call_facts -- --nocapture`
- `cargo test -p effigy scan_tests::dead_code -- --nocapture`
- `target/debug/effigy graph index --json`
- `target/debug/effigy scan dead-code --json`
- `target/debug/effigy scan boundary-violations --json`
- `cargo test -p effigy json_contract_tests::scan_contract_tests -- --nocapture`
- `cargo test -p effigy scan_tests::boundary_violations -- --nocapture`
- `cargo clippy -p effigy-codegraph -p effigy-builtin --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: Effigy suppressed all dead-code symbol findings because the scan
  reported 6,497 mostly noisy findings without suppression.
- Current: no repo-wide symbol suppression; dead-code reports 1,178 advisory
  findings after scanner/indexer fixes.
- Remaining open: decide whether residual findings are another graph-precision
  tranche or targeted cleanup.

## Next Task

Planning checkpoint: decide whether the remaining advisory findings need another
graph-precision tranche or targeted code cleanup.
