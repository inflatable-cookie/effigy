# Env Schema Secret Output Audit

Status: complete
Created: 2026-03-10
Roadmap: g01.025
Batch: secret-output-audit

## Summary

- Audited the remaining env-schema output surfaces for secret leakage.
- Added proof that resolved-env debug formatting masks secrets.
- Added proof that JSON-mode runner failures keep sensitive env-schema validation values redacted.

## Changes

- Added `resolve_debug_output_redacts_secret_values` in `src/env_schema/resolver/tests.rs`.
- Added `cli_catalog_task_json_mode_env_schema_sensitive_validation_redacts_error_message` in `tests/cli_output_tests/command_behavior_tests.rs`.
- Updated the env-schema guide, changelog, and roadmap so the output-surface audit item is now complete.

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`
- Movement: baseline `text-mode task failures were redacted, but JSON envelopes and generic debug surfaces still lacked explicit proof` -> current `normal env-schema reporting surfaces now have direct regression coverage for secret redaction`
- Remaining gap: `only the strict drop-time zeroization proof remains open`

## Validation Performed

- command: `cargo fmt --all -- --check`
  - result: pass
- command: `cargo test resolve_debug_output_redacts_secret_values -- --nocapture`
  - result: pass
- command: `cargo test cli_catalog_task_json_mode_env_schema_sensitive_validation_redacts_error_message -- --nocapture`
  - result: pass
- command: `git diff --check`
  - result: pass

## Risks

- This audit covers the normal text, debug, and JSON command-envelope paths exercised today; future output modes or diagnostics must maintain the same redaction discipline.
- The remaining roadmap item is still the stronger unsafe drop-time zeroization proof, which this batch intentionally does not claim to satisfy.

## Next Task

- Implement the final `g01.025` decision batch: either add a defensible unsafe drop-time zeroization test for `SecretString` or explicitly mark that requirement deferred/out of scope and close the roadmap with that rationale.
