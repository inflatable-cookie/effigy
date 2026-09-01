# `effigy-containers` Environment-Lock Papercut Closeout

Status: complete
Created: 2026-09-01
Roadmap: none (bounded papercut)
Batch: papercuts-env-lock-audit
Handoff: `20260901-121205-papercuts-env-lock-audit.md`

## Summary

- Audited the complete `effigy-containers` test surface for reads and mutations
  of `HOME`, `PATH`, and `EFFIGY_COMPOSE_BACKEND`.
- Closed the race by making the existing `crate::test_env_lock()` the
  precondition for every direct or helper-hidden process-global read that can
  overlap a mutating test.
- No production environment behavior, public API, runtime semantics, catalog
  content, release surface, or workflow changed.

## Changes

- Locked unguarded backend and Colima command tests in `colima/tests.rs` and
  `compose/tests.rs`.
- Locked the direct `HOME` assertions in `mount_spec.rs` and
  `policy_support/generated_compose.rs`.
- Locked bundle-backed policy tests whose `effigy_manifest::load_user_config()`
  path reads `HOME` indirectly in `tests/compose.rs` and `tests/policies.rs`.
- Kept thread-local host-home and Effigy-home override paths unchanged.

## Vision Target Delta

- Primary tags: `MAINT`
- Movement: intermittent process-global test state -> audited and serialized
  `effigy-containers` env-read surface
- Remaining gap: None for this papercut; the separate open queue remains
  unchanged.

## Validation Performed

- `effigy test --plan` — resolved the repository default to
  `cargo nextest run --workspace`.
- `cargo nextest run -p effigy-containers` — five repeated runs under
  nextest's parallel test scheduling; each passed 230/230 tests with no skips.
- `effigy qa` — passed: 3,625/3,625 workspace tests passed, with one existing
  leaky-test signal and one skipped test; documentation and JSON contract
  checks passed.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets -- -D warnings` — passed with the existing
  `proc-macro-error2` future-incompatibility dependency warning and no clippy
  diagnostics.
- `git diff --check` — passed.
- Northstar Rust closeout — passed; compiler and focused-test evidence passed,
  with the same dependency warning recorded for repository clippy.

## Risks

- Future tests that read these variables through a new helper must join the
  same lock; the inventory and shared lock are the recurrence boundary.
- Effigy’s auto-detected workspace test command does not narrow package scope
  when passed a package argument, so the focused proof used direct nextest
  after confirming the Effigy plan.

## Next Task

- Return to planning for official catalog-pack publication and concrete-asset
  cutover. Do not change the active queue here.
