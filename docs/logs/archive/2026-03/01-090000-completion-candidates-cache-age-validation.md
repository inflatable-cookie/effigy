# Completion Candidates Cache Age Validation

Date: 2026-03-01
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add cache freshness telemetry (`cache_age_ms`) to completion candidates JSON on cache hits.

## Changes
- `effigy.completion.candidates.v1` now includes `cache_age_ms`.
- `cache_age_ms` is populated only when `cache_state=hit`; miss states return `null`.
- Existing fields (`cache_hit`, `cache_state`, `manifest_count`) are unchanged.

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
- Initial call: `cache_state=miss_initial`, `cache_age_ms=null`
- Immediate rerun: `cache_state=hit`, `cache_age_ms` numeric
- TTL miss and manifest-change miss remain `cache_age_ms=null`

## Risks / Follow-ups
- `cache_age_ms` reflects in-process cache age only; it is not persisted across CLI invocations.

## Next
- Add an optional `cache_ttl_ms` field so clients can reason about when a hit is near expiry.
