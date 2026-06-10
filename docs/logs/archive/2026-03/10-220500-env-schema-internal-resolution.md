# Env Schema Internal Resolution

Status: complete
Created: 2026-03-10
Roadmap: g01.025
Batch: internal-resolution

## Summary

- Exposed resolved env-schema values to Effigy's internal run-step env resolution path.
- Made task-ref expansion and configured built-in test suite env resolution consume env-schema values, not just child-process injection.
- Added focused regressions across unit, run-array, task-ref, and built-in test paths.

## Changes

- Added shared catalog env-schema resolution support in `src/runner/env_schema_support.rs`.
- Updated run-sequence env resolution to consult env-schema values after process env and before raw dotenv fallback.
- Updated task-ref rendering so referenced tasks inherit their catalog's env-schema-backed task env.
- Updated built-in test suite env/setup/teardown planning to use the same internal env-schema-aware resolution path.
- Updated docs, changelog, and roadmap state for the runtime/internal-resolution batch.

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`
- Movement: baseline `env-schema values were only injected into spawned task processes` -> current `Effigy's own run-array, task-ref, and builtin test planning paths can consume resolved env-schema values internally`
- Remaining gap: `advanced validation coverage for string constraints and pattern matching is still open`

## Validation Performed

- command: `cargo fmt --all -- --check`
  - result: pass
- command: `cargo test apply_from_step_profile_resolution_uses_env_schema_defaults_before_dotenv -- --nocapture`
  - result: pass
- command: `cargo test run_manifest_task_run_array_env_resolution_fallback_contract_table -- --nocapture`
  - result: pass
- command: `cargo test run_manifest_task_run_array_task_reference_env_contract_table -- --nocapture`
  - result: pass
- command: `cargo test run_manifest_task_builtin_test_resolves_configured_suite_env_from_env_schema -- --nocapture`
  - result: pass

## Risks

- Internal env-schema availability currently follows the plain-value path; sensitive values remain spawn-time only to avoid leaking secrets into shell-wrapped command strings.
- Cross-catalog task-ref/env resolution re-resolves env-schema per catalog on demand; this is correct but not yet cached at a broader execution-context level.

## Next Task

- Implement the next `g01.025` validation batch: add parser + validator support for string constraints and pattern matching (`@pattern`, min/max length), then cover the new validation contract end to end.
