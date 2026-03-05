# DAG, Locking, and Onboarding Release Note

Date: 2026-02-28
Owner: Platform
Related roadmap: 010 - DAG Lock and Policy Baseline; 011 - Watch Mode, Init, and Migrate (Phase 1)

## Summary

Effigy now provides a complete orchestration baseline for graph execution, lock safety, and onboarding workflows.

This release combines:
- DAG execution + validation + policy controls (`timeout`, `retry`, `fail_fast`).
- Filesystem lock scopes with stale-lock recovery and operator unlock flows.
- Phase-1 watch mode (`effigy watch`) with owner policy and watch-target lock interop.
- First-party onboarding commands: `effigy init` and `effigy migrate`.

## What Changed

- Added DAG-capable task execution while preserving existing linear chains.
- Added deterministic graph validation for cycles and missing dependencies.
- Added dependency-aware scheduling with bounded concurrency.
- Added node-level execution policy (timeout/retry/fail-fast behavior).
- Added lock scopes and collision diagnostics with remediation guidance.
- Added `effigy unlock` for explicit scope unlock and `--all` recovery.
- Added watch owner lock scope: `task:watch:<target>`.
- Added `effigy init` scaffold generation with schema-valid defaults.
- Added `effigy migrate` package script preview/apply flow (non-destructive).

## JSON Contracts

This release relies on the existing command envelope and extends command payload coverage:
- Envelope: `effigy.command.v1`
- Runner payloads:
  - `effigy.task.run.v1`
  - `effigy.watch.v1`
  - `effigy.unlock.v1`
  - `effigy.init.v1`
  - `effigy.migrate.v1`

## Operator Examples

```bash
effigy build
effigy watch --owner effigy --once test
effigy watch --owner effigy --max-runs 3 --debounce-ms 500 test

# Clear lock contention explicitly
effigy unlock task:watch:test
effigy unlock workspace
effigy unlock --all

# Onboarding helpers
effigy init
effigy init --dry-run
effigy migrate
effigy migrate --apply

# JSON output for CI/tooling
effigy --json watch --owner effigy --once test
effigy --json unlock --all
effigy --json init --dry-run
effigy --json migrate --apply
```

## Compatibility

- Existing linear task syntax remains valid.
- Managed multiprocess config remains supported and unchanged:
  - `[tasks.<name>] concurrent = [...]` (default profile)
  - `[tasks.<name>.profiles.<profile>] concurrent = [...]` (profile overrides)
- Locking introduces intentional collision blocking across scopes; this is runtime safety behavior, not a schema break.

## Validation

- `cargo test run_manifest_task_builtin_watch_ -- --test-threads=1` passed.
- `cargo test run_manifest_task_builtin_init_ -- --test-threads=1` passed.
- `cargo test run_manifest_task_builtin_migrate_ -- --test-threads=1` passed.
- `cargo test builtin_watch_bounded_json_contract_has_versioned_shape -- --test-threads=1` passed.
- `cargo test builtin_init_json_contract_has_versioned_shape -- --test-threads=1` passed.
- `cargo test builtin_migrate_json_contract_has_versioned_shape -- --test-threads=1` passed.
- `cargo test cli_json_mode_watch_ --test cli_output_tests -- --test-threads=1` passed.

## Follow-up

- Phase-2 work can now build on this baseline for expanded watch ergonomics and broader migration source support.
