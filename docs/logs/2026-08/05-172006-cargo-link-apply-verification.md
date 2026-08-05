# Cargo Link Apply And Verification

Status: complete
Created: 2026-08-05
Roadmap: `g08.020`
Batch: `1055`

## Summary

Shipped `effigy deps link cargo`: exact plan application, full local closure
verification, post-verification desired state, bounded rollback, and shared
text/JSON operation reporting.

## Changes

- composed Cargo library/consumer inventory and full-closure planning behind
  one `effigy-deps` operation
- checked every planned file's exact before-state before the first write
- applied config and ignore deltas atomically; persisted the ledger only after
  Cargo metadata and tree proved every workspace/crate pair
- retained exact committed Git sources beside planned and observed local paths
- added explicit dry-run, applied, apply-failed, and verification-failed
  outcomes with rollback evidence
- rolled back only unchanged Effigy-applied config/ignore content on failure;
  never restored or replaced lockfiles
- proved flat and nested real Git-dependency fixtures, complete two-crate
  closure, idempotent refresh, missing-block repair, stale-plan refusal, and
  verification-failure rollback
- exposed text and `effigy.deps.link.v1` JSON reports with exact before/after
  content and prominent tracked-lock warnings
- added a live Cargo-link fixture to the JSON contract checker

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: Cargo linking stopped at a pure plan -> one command now applies and
  proves the full local closure before recording desired state
- Remaining gap: Cargo unlink and committed-source/lock recovery remain in
  ready card `1056`

## Validation Performed

- focused Cargo apply/precondition/rollback/idempotence tests
  - result: passed
- real flat and nested Cargo Git-dependency integration fixtures
  - result: passed; every matching direct/transitive crate resolved locally
- focused deps CLI text/JSON/dry-run tests
  - result: passed
- `cargo test -p effigy-deps`
  - result: 44 unit tests, 2 real Cargo integration tests, and doc tests passed
- `cargo clippy -p effigy --all-targets -- -D warnings`
  - result: passed
- `effigy qa:ci:fast`
  - result: 1,619 tests passed, 1 skipped; 2 existing leaky tests reported
- `effigy qa:ci:json`
  - result: passed; `effigy.deps.link.v1` selected and validated
- `effigy qa:docs`
  - result: passed
- `effigy scan god-files --path crates/effigy-deps/src/cargo_apply.rs --show-warnings`
  - result: no findings
- `cargo fmt --all -- --check`
  - result: passed
- `git diff --check`
  - result: passed

## Risks

- Cargo verification may rewrite affected lockfiles while a patch is active;
  the operation reports them as do-not-commit state and never restores them
- unlink and clean committed-source recovery remain deliberately unavailable
  until `1056`

## Next Task

Execute ready batch card `1056`.
