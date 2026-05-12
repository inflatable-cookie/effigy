# 2026-04-15 Release Wrapper Retirement And Native Cutover

## Summary

Retired Effigy's remaining compatibility-only shell wrappers so the repo now
points directly at native Effigy tasks and built-in release commands instead of
preserving legacy `.sh` entrypoints for migration safety.

## Shipped

- removed compatibility-only shell entrypoints:
  - `scripts/install-local-bin-links.sh`
  - `scripts/check-release-smoke.sh`
  - `scripts/check-release-gates.sh`
  - `scripts/check-release-install-from-tag.sh`
  - `scripts/prepare-release.sh`
- removed now-unused Rhai wrapper shims:
  - `scripts/check-release-gates.rhai`
  - `scripts/check-release-install-from-tag.rhai`
- switched live repo config and docs to native paths:
  - `effigy link:local`
  - `smoke:release`
  - `effigy release gates`
  - `effigy release verify-install`
  - `effigy release prepare`
- kept only the honest shell/platform boundaries:
  - `scripts/check-distribution-first-publish.sh`
  - `scripts/check-linux-glibc-floor.sh`
  - `scripts/effigy-dev`

## Why

Leaving compatibility wrappers in place after Rhai and built-in release
surfaces were already established kept the repo in an in-between state. This
batch finishes the cutover so Effigy now dogfoods the stricter rule it is meant
to promote elsewhere: shell scripts stay only when they still do real shell or
platform work.

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Decide whether the Rhai lane should now pause cleanly on the shipped Effigy
dogfooding boundary until an external pilot repo becomes safe again.
