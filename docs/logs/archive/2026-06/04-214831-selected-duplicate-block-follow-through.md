# Selected Duplicate Block Follow-Through

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1042`

## Summary

Completed the selected duplicate-block reduction pass.

The high duplicate findings from the sweep are removed. The remaining
duplicate-block findings are warning-only and are intentionally left as a
maintenance queue rather than broad cleanup.

## Changes

- Extracted shared graph-aware scan helpers for dead-code and validation-gap
  scans:
  - file-role classification
  - supported-language mapping
  - glob compilation
  - first-symbol line resolution
- Preserved the one known semantic difference between the two scanners:
  dead-code treats crate-root `lib.rs` as script-like entrypoint code, while
  validation-gaps keeps that file role as implementation.
- Split container and release help topic option rows into named slices so large
  topic specs no longer form high duplicate blocks.
- Added an internal `effigy-runtime` test-support helper for generated
  `EffectiveContainerPolicy` fixtures and reused it across runtime read,
  discovery, and data-planning tests.

## Residual Findings

`effigy scan duplicate-blocks --json` now reports:

- critical: 0
- high: 0
- warning: 105

Deferred warning-only groups:

- catalog service declaration fixtures remain explicit because the repeated
  service shapes are often the behavior under test
- remaining container policy builders cross crate/package boundaries and need a
  separate test-support ownership decision
- help-topic warning blocks remain below the high threshold and are easier to
  address with a broader help-topic struct follow-up if they become noisy
- codegraph language-emitter and Rhai JSON conversion duplication is outside
  this card's selected owner groups

## Validation

- `cargo fmt --all`
- `cargo test -p effigy-builtin scan -- --nocapture`
- `cargo test -p effigy-cli help -- --nocapture`
- `cargo test -p effigy-runtime read -- --nocapture`
- `cargo test -p effigy-runtime data::planning -- --nocapture`
- `cargo test -p effigy-runtime container_manager -- --nocapture`
- `cargo test -p effigy-runtime shell -- --nocapture`
- `cargo clippy -p effigy-builtin -p effigy-cli -p effigy-runtime --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy scan duplicate-blocks --json`
- `effigy test --plan`

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: duplicate-block scan reported 2 high findings and 112 warning
  findings for this follow-up tranche.
- Current: duplicate-block scan reports 0 high findings and 105 warning
  findings, with residual groups classified for future maintenance.
- Remaining open: boundary/dead-code scan self-adoption in `g08.009`.

## Next Task

Run ready card `1043`.
