# Env Schema Config Alignment

Status: complete
Created: 2026-03-10
Roadmap: g01.025
Batch: config-alignment

## Summary

- Added focused runtime coverage for `[env_schema]` manifest behavior: `enabled`, `schema`, and `exec_timeout`.
- Added config guardrails so empty schema paths and zero-second exec timeouts fail before task execution.
- Reconciled the roadmap configuration checklist against the behavior Effigy already ships under `[env_schema]`.

## Changes

- Added config validation in `src/runner/env_schema_support.rs` for whitespace-only `schema` values and `exec_timeout = 0`.
- Extended `src/tests/runner_tests/runner_core_tests/task_env_tests.rs` with runtime coverage for:
  - `enabled = false` skipping schema loading
  - `enabled = true` requiring the schema file
  - manifest-level `schema` overrides
  - invalid `exec_timeout`
  - invalid empty `schema`
- Updated the env-schema guide, changelog, and roadmap state to reflect the validated configuration contract.

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`
- Movement: baseline `env-schema config existed but section-7 roadmap coverage and validation were only partially proven` -> current `manifest configuration behavior is explicitly covered and obvious invalid values fail fast`
- Remaining gap: `the largest open roadmap items are now the remaining secret-output audit and a few older parser/resolution checklist entries that need explicit roadmap reconciliation`

## Validation Performed

- command: `cargo fmt --all -- --check`
  - result: pass
- command: `cargo test run_manifest_task_env_schema_config_ -- --nocapture`
  - result: pass
- command: `cargo test run_manifest_task_env_schema_override -- --nocapture`
  - result: pass
- command: `git diff --check`
  - result: pass

## Risks

- Configuration validation currently happens when env-schema resolution is consulted, not at manifest deserialization time, so misconfiguration is still surfaced at task invocation rather than manifest parse.
- The roadmap section still references the older `[env]` naming while the shipped product uses `[env_schema]`; that mismatch is documented but should stay explicit in future cleanup.

## Next Task

- Implement the next `g01.025` cleanup batch: reconcile the remaining roadmap checklist entries that are already effectively shipped under different internal names or module paths, and add any missing focused tests needed to close those items without pretending the implementation matches the original sketch literally.
