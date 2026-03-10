# Env Schema Secret Redaction And Zeroizing

Status: complete
Created: 2026-03-10
Roadmap: g01.025
Batch: security-redaction

## Summary

- Switched `SecretString` to store secrets in `zeroize::Zeroizing<String>`.
- Redacted sensitive env-schema validation values so task/runtime errors no longer echo secret contents.
- Added focused tests around practical zeroization semantics and runtime redaction behavior.

## Changes

- Updated `src/env_schema/secret.rs` to wrap the inner string with `Zeroizing<String>`.
- Added test-only helpers that inspect the still-owned buffer after zeroization so secret tests can verify the buffer contents are cleared without reading freed memory.
- Changed `src/env_schema/validator.rs` / `src/env_schema/error.rs` so `ValidationError` redacts `actual` values for `@sensitive` entries at creation time.
- Added focused unit and runtime regressions for redaction and zeroization behavior.
- Updated guide, changelog, and roadmap security checklist state for the items this batch now satisfies.

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`
- Movement: baseline `sensitive env values were protected in display wrappers but validation errors still captured raw values` -> current `sensitive validation failures are redacted and secret storage uses `Zeroizing<String>` with practical buffer-clearing coverage`
- Remaining gap: `the remaining security checklist items are the broader secret-output audit and the stricter drop-time verification target`

## Validation Performed

- command: `cargo fmt --all -- --check`
  - result: pass
- command: `cargo test env_schema::secret::tests -- --nocapture`
  - result: pass
- command: `cargo test env_schema::validator::tests::sensitive_validation_error_redacts_actual_value -- --nocapture`
  - result: pass
- command: `cargo test run_manifest_task_env_schema_sensitive_validation_redacts_value -- --nocapture`
  - result: pass
- command: `git diff --check`
  - result: pass

## Risks

- The zeroization test is intentionally practical rather than proving post-drop memory state; it verifies the owned buffer is cleared before deallocation, not that freed memory remains unreadable.
- Secret-output auditing is now stronger for validation errors, but other future env-schema diagnostics still need the same redaction discipline if new error paths are added.

## Next Task

- Implement the next `g01.025` module-integration batch: round out the public env-schema API (`validate_env`/export helpers and naming cleanup around resolved values), then reconcile the remaining section-5 checklist items against the actual shipped surface.
