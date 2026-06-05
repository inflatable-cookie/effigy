# Dead-Code Descriptor-Root Precision

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1047`

## Summary

Completed the descriptor/dispatch root precision slice for the dead-code scan.

The scanner now treats private Rust functions as roots when there is source
evidence that they are assigned into descriptor or dispatch structures. This
covers function-pointer field values and typed `fn` dispatch tables. Ordinary
private helper functions still report when they have no inbound references.

## Changes

- Added Rust descriptor/dispatch root classification to the dead-code scan.
- Added a regression fixture proving:
  - descriptor field functions do not report
  - typed dispatch-table functions do not report
  - unrelated private helper functions still report
- Kept the rule repo-agnostic: no help-registry or file-path allowlist.

## Result

Before this slice, after `1046`, `target/debug/effigy scan dead-code --json`
reported:

- findings: 521
- isolated files: 5
- unreferenced symbols: 516
- function findings: 307

After this slice:

- findings: 488
- isolated files: 5
- unreferenced symbols: 483
- checked symbols: 2,498
- function findings: 274

The `crates/effigy-cli/src/help/registry.rs` descriptor-wrapper group dropped
out of the largest finding groups.

Largest remaining groups:

- `crates/effigy-release/src/render_json.rs`: 21
- `crates/effigy-runtime/src/data/volumes.rs`: 13
- `crates/effigy-tasks/src/listing.rs`: 12
- `crates/effigy-containers/src/policy_support/generated_compose.rs`: 12
- `src/runner/deploy_command/transaction.rs`: 11
- `src/runner/deploy_command/derive.rs`: 11

## Remaining Queue

The next residual classes are:

- DTO/render/config structs and enums that are data-shape roots
- associated and path-qualified Rust call matching
- remaining private helper pockets that may be real cleanup
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
- Baseline: residual dead-code scan had 521 findings after trait/API surface
  handling, including 307 function findings.
- Current: residual findings are 488, with function findings reduced to 274
  while private non-dispatch helpers remain visible.
- Remaining open: decide the next `g08.009` batch from DTO/render models,
  associated-call matching, real cleanup, or isolated-file inspection.

## Next Task

Planning checkpoint: decide the next `g08.009` residual dead-code batch.
