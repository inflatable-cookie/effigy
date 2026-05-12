# 2026-04-15 Effigy Wrapper Convergence Boundary Implementation

## Summary

Closed the remaining low-risk Effigy-only wrapper convergence gap by moving the
last thin shell-glue entrypoints onto the same Rhai-backed compatibility
launcher pattern already used for release wrappers.

## Shipped

- converted `scripts/install-local-bin-links.sh` into a minimal launcher for
  `scripts/install-local-bin-links.rhai`
- converted `scripts/check-release-smoke.sh` into a minimal launcher for
  `scripts/check-release-smoke.rhai`
- updated release/Rhai docs so these two scripts are no longer described as
  permanent shell boundaries
- left the genuine shell/platform boundaries explicit:
  - `scripts/check-distribution-first-publish.sh`
  - `scripts/check-linux-glibc-floor.sh`
  - `scripts/effigy-dev`
  - `scripts/prepare-release.sh`

## Why

The remaining implementation question in `g02.004` was no longer “can Rhai do
this?” It was whether the repo still had easy wrapper-boundary cleanup left.
These two scripts were already backed by real Rhai logic, so leaving them as
shell implementations would have kept the lane artificially incomplete.

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Decide whether the Rhai lane should now pause cleanly on this Effigy dogfooding
boundary until an external pilot repo becomes safe again.
