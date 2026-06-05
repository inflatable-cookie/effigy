# Dead-Code Data-Shape Root Precision

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1048`

## Summary

Completed the DTO/render/config data-shape precision batch for the dead-code
scan.

The scanner now credits private Rust structs and enums when their type names
are referenced outside their declaration in type or construction contexts. This
reduces data-shape false positives without blanket suppression: unused private
structs and enums still report when there is no type-reference evidence.

## Changes

- Added Rust type-reference root classification for private structs and enums.
- Kept the rule repo-agnostic and source-evidence based.
- Added a regression fixture proving:
  - referenced payload structs do not report
  - referenced row enums do not report
  - unused private structs still report
  - unrelated private helper functions still report

## Result

Before this slice, after `1047`, `target/debug/effigy scan dead-code --json`
reported:

- findings: 488
- isolated files: 5
- unreferenced symbols: 483
- function findings: 274
- struct findings: 174
- enum findings: 31

After this slice:

- findings: 285
- isolated files: 5
- unreferenced symbols: 280
- checked symbols: 2,293
- function findings: 274
- struct findings: 2
- enum findings: 0

Largest remaining groups:

- `crates/effigy-ui/src/plain_renderer/mod.rs`: 10
- `crates/effigy-codegraph/src/storage.rs`: 10
- `crates/effigy-manifest/src/bundles.rs`: 8
- `crates/effigy-changelog/src/parser.rs`: 8
- `crates/effigy-builtin/src/config/docs/tasks.rs`: 8

Remaining non-function findings:

- `crates/effigy-builtin/src/completion/prompt.rs`: `Pipe` trait
- `crates/effigy-builtin/src/scan/execution/core/api.rs`: two methods
- `crates/effigy-env/src/validator.rs`: `Validator` trait
- `crates/effigy-managed/src/plan.rs`: `ConcurrentResolvedProcess` struct
- `crates/effigy-tui/src/multiprocess/mod.rs`: `SessionRuntime` struct

## Remaining Queue

The next residual classes are:

- associated and path-qualified Rust call matching
- remaining function helper pockets that may be real cleanup
- a small set of trait/method/type findings that need manual classification
- isolated-file findings that still need manual inspection

## Validation

- `cargo test -p effigy scan_tests::dead_code -- --nocapture`
- `cargo build -p effigy`
- `cargo clippy -p effigy-builtin --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `target/debug/effigy graph index --json`
- `target/debug/effigy scan dead-code --json`
- `cargo test -p effigy scan_contract_tests::dead_code -- --nocapture`
- `cargo fmt --all -- --check`

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: residual dead-code scan had 488 findings after descriptor/dispatch
  root handling, including 174 struct and 31 enum findings.
- Current: residual findings are 285, with struct findings reduced to 2 and
  enum findings reduced to 0 while unused private types remain visible.
- Remaining open: decide the next `g08.009` batch from associated-call
  matching, focused real cleanup, remaining non-function classification, or
  isolated-file inspection.

## Next Task

Planning checkpoint: decide the next `g08.009` residual dead-code batch.
