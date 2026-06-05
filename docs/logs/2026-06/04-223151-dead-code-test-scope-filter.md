# Dead-Code Test-Scope Filter

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1045`

## Summary

Completed the first residual dead-code precision slice.

Rust `#[test]` functions are now treated as test entrypoints rather than
production dead-code candidates. Helpers inside `tests::` modules still remain
eligible findings when they are unused.

## Changes

- Added Rust test-entrypoint filtering in the dead-code scan.
- Added a regression fixture proving:
  - private production helpers still report
  - unused private test helpers still report
  - `#[test]` functions do not report
- Kept isolated-file and private production symbol findings visible.

## Result

Before this slice, after `g08.009`, `target/debug/effigy scan dead-code --json`
reported:

- findings: 1,178
- isolated files: 5
- unreferenced symbols: 1,173

After this slice:

- findings: 661
- isolated files: 5
- unreferenced symbols: 656
- checked symbols: 2,660
- remaining `tests::` helper findings: 51

The remaining largest path groups are:

- `crates/effigy-cli/src/help/registry.rs`: 27
- `crates/effigy-builtin/src/scan/execution/core/api.rs`: 21
- `crates/effigy-release/src/render_json.rs`: 21
- `crates/effigy-ui/src/renderer.rs`: 16
- `crates/effigy-builtin/src/ports.rs`: 15

## Remaining Queue

Remaining findings still look mostly graph precision oriented:

- trait and impl method surfaces
- path-qualified and associated Rust call matching
- dispatch-table and descriptor-owned helper paths
- unused test helpers that may be real cleanup
- possible small real cleanup in low-count helper files after graph gaps are
  reduced

## Validation

- `cargo test -p effigy scan_tests::dead_code -- --nocapture`
- `cargo clippy -p effigy-builtin --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `target/debug/effigy scan dead-code --json`

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: residual dead-code scan had 1,178 findings after Rust signal
  correction.
- Current: residual findings reduced to 661 by filtering Rust `#[test]`
  entrypoints while keeping unused test helpers visible.
- Remaining open: decide next precision slice for trait/impl and
  path-qualified call handling.

## Next Task

Planning checkpoint: decide the next residual dead-code precision slice.
