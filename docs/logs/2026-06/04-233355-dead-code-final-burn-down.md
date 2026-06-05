# Dead-Code Final Burn-Down

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1050`

## Summary

Completed the current dead-code residual burn-down.

The scan now handles the remaining false-positive classes seen in Effigy:
function references hidden in macro bodies or function-value arguments, binary
entrypoints, serde default helpers, private traits referenced by impl headers,
nested crate module paths, and cross-file Rust references for otherwise
candidate symbols.

After those fixes, the remaining findings were genuine dead artifacts and were
removed:

- `crates/effigy-doctor/src/error.rs`: removed `_typecheck_path_types`
- `src/runner/model.rs`: deleted undeclared runner model shim

## Result

Before this final batch, after `1049`, `target/debug/effigy scan dead-code
--json` reported:

- findings: 196
- isolated files: 4
- unreferenced symbols: 192

After the final burn-down:

- findings: 0
- isolated files: 0
- unreferenced symbols: 0
- checked files: 797
- checked symbols: 14

## Scanner Improvements

- Added candidate-only repository source reference checks.
- Added binary `main` entrypoint handling.
- Added serde `default = "helper"` function reference handling.
- Added function reference handling for call-like, turbofish, macro-body, and
  argument-value contexts.
- Added private trait reference handling for impl headers.
- Fixed module-file matching for nested `*/src/` crate paths.

## Validation

- `cargo test -p effigy scan_tests::dead_code -- --nocapture`
- `cargo build -p effigy`
- `cargo clippy -p effigy-builtin --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `target/debug/effigy graph index --json`
- `target/debug/effigy scan dead-code --json`

## Validation Note

`cargo clippy -p effigy-doctor --all-targets -- -D warnings ...` was attempted
because the deletion touched `effigy-doctor`, but it failed on an existing
`items_after_test_module` warning in `crates/effigy-doctor/src/environment.rs`.
That warning is unrelated to this dead-code cleanup.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: residual dead-code scan had 196 findings after Rust impl/call
  precision.
- Current: residual dead-code scan has 0 findings after scanner precision fixes
  and two confirmed deletions.
- Remaining open: decide separately whether to make dead-code findings a gate
  after a stability period.

## Next Task

No current dead-code residual batch remains.
