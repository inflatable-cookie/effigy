# Rhai Feature Descriptor Seam

Date: 2026-06-04
Roadmap: `g08.009`
Batch card: `1039`

## Summary

Added a typed Rhai feature descriptor seam for command-backed helpers. Surface
rendering now reads feature metadata from descriptors, and runner coverage
uses descriptor dispatch ownership instead of a hard-coded test exception.

No Rhai helper was removed. Script grammar and command behavior stayed
unchanged.

## Changes

- Added `RhaiFeatureDescriptor` with feature id, option style, safety, and
  dispatch ownership.
- Added descriptor accessors in `crates/effigy-rhai/src/surface.rs`.
- Rendered command-backed surface rows from descriptors.
- Replaced the runner coverage test's `state.capture_set` exception with
  explicit `HostHandled` descriptor ownership.
- Switched unknown-feature dispatch from raw `FEATURE_NAMES` membership to the
  descriptor lookup.

## Deferred Cleanup

- `FEATURE_NAMES` remains as a compatibility list for this slice. Descriptor
  coverage now proves it matches `FEATURE_DESCRIPTORS`; a later cleanup can
  collapse remaining direct consumers when that is worth the churn.
- Host module registration remains explicit. This card deliberately did not
  generate Rhai modules or helper overloads from descriptors.

## Validation

- `cargo fmt --all`: pass
- `cargo test -p effigy-rhai`: pass
- `cargo test script_command::tests::every_registered_rhai_feature_has_a_runner_dispatch_branch -- --nocapture`: pass
- `cargo test rhai_surface -- --nocapture`: pass
- `effigy rhai surface --json`: pass
- `cargo clippy -p effigy-rhai --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`: pass

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline -> current: Rhai command-helper metadata now has a descriptor source
  for surface rendering and dispatch coverage.
- Open: container `up` still mixes bring-up phases and is next.

## Next Task

Run `1040`.
