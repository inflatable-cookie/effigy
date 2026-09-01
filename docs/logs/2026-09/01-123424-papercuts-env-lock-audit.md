# `effigy-containers` Environment-Lock Papercut Closeout

Status: complete
Created: 2026-09-01
Roadmap: none (bounded papercut)
Batch: papercuts-env-lock-audit
Handoff: `20260901-121205-papercuts-env-lock-audit.md`

## Summary

- Follow-up to the review finding on PR 69 at exact head
  `c9ee314b7080456e40565baed8b1170c13a2afaa`.
- The review found two missed execution paths: direct and generated runtime-DNS
  policy tests called `load_container_policy` without the shared lock.
- Re-audit traced those calls through `policy/load.rs:341-349` to
  `runtime/dns.rs:139-145`, where the Colima fallback reads `HOME` after
  discovering compose services. The same path covers every confirmed non-empty
  Colima policy caller, plus inline workspace policy loading.
- The corrected inventory covers semantic reads of `HOME`, `PATH`, and
  `EFFIGY_COMPOSE_BACKEND`, not only direct `std::env` sites. No production
  environment behavior, public API, runtime semantics, catalog content,
  release surface, or workflow changed.

## Review correction and semantic inventory

- `HOME`: `runtime/dns::colima_home_dir` is reached by
  `materialize_runtime_dns_override`; `workspace::host_home_dir` is reached by
  workspace mount rewriting; `policy_support::effigy_home_dir` and
  `effigy_manifest::load_user_config` use thread-local test overrides where
  callers provide them, otherwise they fall back to `HOME`; Colima profile
  preparation and cleanup retain their existing direct reads. All affected
  test callers are locked; direct HOME assertions were already locked.
- `PATH` and `EFFIGY_COMPOSE_BACKEND`:
  `ContainerBackendDetection::from_env_and_path` reaches
  `manager::backend_override_from_env` and
  `manager::resolve_host_cli_program_path`. The compose and Colima test callers
  that execute those paths already hold the shared lock. `ContainerManager::defaults`
  itself only builds the backend registry and is not an environment read.
- Thread-local Effigy-home, user-config-home, and host-home overrides remain
  unchanged. Tests whose control flow returns before a named-variable read
  remain unlocked and parallel.

## Changes

- Added the shared lock to the two review-named runtime-DNS tests and the other
  confirmed non-empty Colima policy callers in `tests/compose.rs`,
  `tests/policies.rs`, and `tests/volumes_reports.rs` (28 new test
  preconditions).
- Retained the existing locks for backend/Colima command tests, direct HOME
  assertions, bundle-backed policy resolution, and the workspace-app policy
  path.
- Kept thread-local host-home and Effigy-home override paths unchanged.

## Vision Target Delta

- Primary tags: `MAINT`
- Movement: intermittent process-global test state -> audited and serialized
  `effigy-containers` env-read surface
- Remaining gap: None for this papercut; the separate open queue remains
  unchanged.

## Validation Performed

- `effigy test --plan` resolved the repository default to
  `cargo nextest run --workspace`.
- `cargo test -p effigy-containers --lib` passed: 230 passed, 0 failed.
- Five repeated `cargo nextest run -p effigy-containers` runs passed under
  parallel scheduling: 230 passed, 0 skipped on each run.
- `effigy qa` passed: 3,625 workspace tests passed (one existing leaky test),
  one test skipped, and the docs, workflow-path, and JSON-contract checks
  passed.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo clippy --all-targets -- -D warnings` passed. The only reported
  warning is the existing dependency future-incompatibility notice for
  `proc-macro-error2 v2.0.1`.
- Northstar Rust-quality closeout passed compiler and focused-test evidence;
  the repository-clippy evidence carries the same dependency warning.
  Receipt snapshot: `8bba20b13040fa847c3a9727bb27183ab932d0eb0cd456838778fd6fdca17a04`.

## Risks

- Future tests that read these variables through a new helper must join the
  same lock; the semantic inventory and shared lock are the recurrence boundary.
- Effigy’s auto-detected workspace test command does not narrow package scope
  when passed a package argument, so the focused proof used direct nextest
  after confirming the Effigy plan.

## Next Task

- Return to planning for official catalog-pack publication and concrete-asset
  cutover. Do not change the active queue here.
