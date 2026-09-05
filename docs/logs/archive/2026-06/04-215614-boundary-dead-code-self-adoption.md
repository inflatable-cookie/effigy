# Boundary And Dead-Code Self-Adoption

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1043`

## Summary

Completed the Effigy self-adoption pass for graph-aware boundary and dead-code
scans.

The boundary scan now checks one real Effigy seam. The dead-code scan is tuned
for isolated-file findings only, because symbol-level Rust reference coverage
is still too noisy for this repo.

Update: the dead-code symbol suppression described here was superseded by
[`04-221542-dead-code-scan-rust-signal-correction.md`](./04-221542-dead-code-scan-rust-signal-correction.md).
Effigy no longer uses repo-wide `allow_symbols = ["*"]`.

## Changes

- Added Effigy-owned boundary layers in `config/scan.toml`:
  - `scan_model`: `crates/effigy-scan/src/**`
  - `scan_builtin_adapter`: `crates/effigy-builtin/src/scan/**`
- Allowed the built-in scan adapter to depend on the scan model.
- Kept the boundary scan advisory:
  - `doctor = false`
  - `fail_on_findings = false`
  - `include_heuristic = false`
- Tuned `scan.dead_code` for Effigy self-use:
  - kept the scan advisory
  - skipped symbol-level findings with `allow_symbols = ["*"]`
  - preserved isolated-file findings as the actionable queue

## Scan Interpretation

`scan boundary-violations` is useful as an advisory seam check now. It should
not become a gate until it has more history across normal development changes.

`scan dead-code` is not a semantic deletion signal for Effigy yet. Rust symbol
findings currently undercount module exports and internal uses, so the repo
config intentionally suppresses symbol findings. Isolated files remain visible
because that bucket is smaller and easier to inspect manually.

## Residual Findings

After tuning, `effigy scan dead-code --json` reports:

- checked files: 794
- checked symbols: 6,482
- skipped allowlisted symbols: 6,482
- findings: 20 isolated files

Sampled isolated-file findings are wired by module declarations or direct local
uses. The next improvement is graph coverage, especially module declaration and
same-crate reference accounting, not code deletion.

`effigy scan boundary-violations --json` reports:

- configured layers: 2
- checked edges: 7
- findings: 0

## Validation

- `effigy graph index --json`
- `cargo test -p effigy scan_tests::boundary_violations -- --nocapture`
- `cargo test -p effigy scan_tests::dead_code -- --nocapture`
- `cargo test -p effigy json_contract_tests::scan_contract_tests -- --nocapture`
- `effigy scan boundary-violations --json`
- `effigy scan dead-code --json`
- `effigy test --plan`
- `effigy qa:docs`
- `git diff --check`

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: boundary scan had no configured Effigy layers; dead-code reported
  6,467 findings, mostly noisy symbol-level warnings.
- Current: boundary scan checks the scan model/adapter seam; dead-code reports
  only the 20 isolated-file findings and records symbol findings as a graph
  coverage gap.
- Remaining open: decide the next g08 tranche from graph coverage and residual
  maintainability evidence.

## Next Task

Planning checkpoint: decide the next `g08` tranche from the completed sweep
evidence.
