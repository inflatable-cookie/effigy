# Dead-Code Trait-Surface Precision

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1046`

## Summary

Completed the trait/API surface precision slice for the dead-code scan.

The scanner now treats Rust trait method declarations and methods implemented
inside `impl Trait for Type` blocks as surface-owned symbols. It still reports
private inherent methods, because those are real cleanup candidates when no
inbound references exist.

## Changes

- Added Rust source-span classification for trait/API method surfaces in the
  dead-code scan.
- Kept the classification scanner-local and repo-agnostic.
- Added a regression fixture proving:
  - public trait declarations do not report
  - trait method declarations do not report as standalone dead code
  - required trait impl methods do not report
  - unused private inherent methods still report
- Left unused test helpers visible.

## Result

Before this slice, after `g08.009`, `target/debug/effigy scan dead-code --json`
reported:

- findings: 661
- isolated files: 5
- unreferenced symbols: 656
- method findings: 92
- trait findings: 2

After this slice:

- findings: 521
- isolated files: 5
- unreferenced symbols: 516
- checked symbols: 2,524
- method findings: 2
- trait findings: 2

Largest remaining groups:

- `crates/effigy-cli/src/help/registry.rs`: 27
- `crates/effigy-release/src/render_json.rs`: 21
- `crates/effigy-runtime/src/data/volumes.rs`: 13
- `src/runner/deploy_command/transaction.rs`: 12
- `crates/effigy-tasks/src/listing.rs`: 12
- `crates/effigy-containers/src/policy_support/generated_compose.rs`: 12

## Remaining Queue

The next residual classes are no longer dominated by trait methods. They are:

- descriptor and dispatch-table owned helpers
- DTO/render/config structs and enums that are data-shape roots
- path-qualified and associated Rust call matching
- unused test helpers that may be real cleanup
- small private helper pockets that can be inspected after graph precision
  classes are exhausted

## Validation

- `cargo test -p effigy scan_tests::dead_code -- --nocapture`
- `cargo build -p effigy`
- `target/debug/effigy graph index --json`
- `target/debug/effigy scan dead-code --json`
- `cargo test -p effigy scan_contract_tests::dead_code -- --nocapture`
- `cargo clippy -p effigy-builtin --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo fmt --all -- --check`

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: dead-code residuals were 661 findings with 92 method findings after
  test-entrypoint handling.
- Current: residual findings are 521, with method findings reduced to 2 while
  private inherent methods remain eligible.
- Remaining open: choose the next precision slice: descriptor/dispatch roots,
  DTO/render models, associated-call matching, or real cleanup.

## Next Task

Planning checkpoint: decide the next residual dead-code precision slice.
