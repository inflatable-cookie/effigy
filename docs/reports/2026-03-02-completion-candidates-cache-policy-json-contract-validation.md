# Completion Candidates Cache Policy JSON Contract Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add end-to-end JSON contract coverage for completion cache TTL source policy fields.

## Changes
- Added JSON contract tests for:
  - `cache_ttl_source=env` with valid env override (`750`)
  - `cache_ttl_source=env_invalid` with malformed env override
- Added scoped env helper in JSON contract tests to avoid cross-test env leakage.

## Validation
- command: `cargo fmt`
  - result: pass
- command: `cargo test builtin_completion_candidates_json_contract_reports_env_ttl_policy -- --test-threads=1`
  - result: pass
- command: `cargo test builtin_completion_candidates_json_contract_reports_invalid_env_ttl_policy -- --test-threads=1`
  - result: pass
- command: `cargo test completion_ -- --test-threads=1`
  - result: pass
- command: `./scripts/check-json-contracts.sh --fast`
  - result: pass

## Evidence
- Valid env override (`750`) yields:
  - `effective_cache_ttl_ms=750`
  - `cache_ttl_source=env`
  - `cache_ttl_ms=750` on hit
- Invalid env override (`not-a-number`) yields:
  - `effective_cache_ttl_ms=2000`
  - `cache_ttl_source=env_invalid`

## Risks / Follow-ups
- Env override tests are process-global and therefore serialized via the shared test lock.

## Next
- Add explicit JSON field docs in `017-json-output-contracts.md` for completion cache policy telemetry keys.
