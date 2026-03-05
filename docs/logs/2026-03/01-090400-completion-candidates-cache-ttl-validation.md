# Completion Candidates Cache TTL Validation

Date: 2026-03-01
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add `cache_ttl_ms` telemetry to completion candidate JSON hits.

## Changes
- `effigy.completion.candidates.v1` now includes `cache_ttl_ms`.
- `cache_ttl_ms` is emitted when `cache_state=hit`.
- Miss states keep `cache_ttl_ms=null`.

## Validation
- command: `cargo fmt`
  - result: pass
- command: `cargo test completion_ -- --test-threads=1`
  - result: pass
- command: `cargo test run_manifest_task_builtin_test_plan_ -- --test-threads=1`
  - result: pass
- command: `./scripts/check-json-contracts.sh --fast`
  - result: pass

## Evidence
- Hit response includes:
  - `cache_state=hit`
  - `cache_age_ms` numeric
  - `cache_ttl_ms=2000`
- Miss responses include:
  - `cache_state=miss_*`
  - `cache_age_ms=null`
  - `cache_ttl_ms=null`

## Risks / Follow-ups
- TTL is currently static (2s) and process-local.

## Next
- Add a manifest/CLI-configurable completion candidate TTL (bounded range) with defaults preserved.
