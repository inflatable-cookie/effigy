# 094 Implement Effigy Rhai Release Wrapper Cluster

Status: archived
Updated: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/archive/004-rust-native-scripting-strict-lane.md`

## Objective

Keep Rhai dogfooding inside Effigy while external pilots are temporarily
deferred by migrating a meaningful cluster of release-validation shell wrappers
onto file-backed Rhai scripts.

## In Scope

- migrate bounded Effigy release helper wrappers such as:
  - `scripts/check-release-gates.sh`
  - `scripts/check-release-install-from-tag.sh`
- keep the migration focused on validation/wrapper surfaces, not actual release
  execution mutation paths
- use the batch to expose any remaining file/process/path capability gaps in
  real operator workflows

## Out Of Scope

- release execution command redesign
- `scripts/prepare-release.sh` replacement if it needs broader mutation helpers
- Keepsake work
- Jetstream work

## Acceptance Criteria

- at least one meaningful release-wrapper cluster is Rhai-backed
- the migrated surfaces remain operator-honest and validated
- the batch leaves one new explicit ready card

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

After this batch, decide whether Effigy dogfooding is now sufficiently broad to
reopen the first external pilot or whether one more Effigy-only Rhai slice is
still the honest move.
