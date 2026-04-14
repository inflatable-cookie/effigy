# Effigy Rhai Release Wrapper Cluster Implementation

Date: 2026-04-15
Roadmap: `g02.004`
Spec: `docs/specs/004-rust-native-scripting-strict-lane.md`
Batch: `094-implement-effigy-rhai-release-wrapper-cluster`

## Summary

Migrated Effigy's release-validation compatibility wrappers onto file-backed
Rhai scripts while keeping the executable shell entrypoints as minimal launch
stubs for CI/docs compatibility.

## What Landed

- added explicit `__rhai-step` CLI support for:
  - `--repo-root`
  - `--task-name`
  - passthrough script args after `--`
- moved `scripts/check-release-gates.sh` logic into:
  - `scripts/rhai/check-release-gates.rhai`
- moved `scripts/check-release-install-from-tag.sh` logic into:
  - `scripts/rhai/check-release-install-from-tag.rhai`
- kept the `.sh` scripts as executable compatibility launchers that call
  `effigy __rhai-step`
- updated wrapper parity tests to prove the Rhai-backed wrappers still delegate
  to the same built-in release commands

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Residual Notes

- `scripts/prepare-release.sh` remains intentionally out of scope because it is
  a broader mutation/backstop path rather than a bounded validation wrapper
- `scripts/check-distribution-first-publish.sh` remains an intentional
  side-effecting shell boundary

## Vision Target Delta

- Primary tags: `OPERATE`, `ADOPT`, `CONTRACT`
- Movement:
  - Effigy Rhai dogfooding expanded from tasks/demos into release-wrapper
    compatibility surfaces
  - wrapper launch semantics now exercise the same native scripting path rather
    than bespoke shell logic
- Remaining open:
  - whether this is enough dogfooding to reopen the first external pilot
