# Completion Candidates Cache State Validation

Date: 2026-03-01
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add explicit cache-state telemetry to `effigy completion candidates --json`.
- Distinguish initial miss, hit, TTL miss, and manifest-change miss.

## Changes
- Added `cache_state` field to `effigy.completion.candidates.v1` payload.
- States shipped:
  - `miss_initial`
  - `hit`
  - `miss_ttl`
  - `miss_manifest_change`
- Preserved existing `cache_hit` boolean for compatibility.

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
- First call in fresh repo: `cache_state=miss_initial`
- Immediate rerun unchanged: `cache_state=hit`
- Rerun after >2s TTL: `cache_state=miss_ttl`
- Rerun after manifest mtime/content update: `cache_state=miss_manifest_change`

## Risks / Follow-ups
- `cache_state` is process-local telemetry; it does not represent cross-process persistence behavior.

## Next
- Add `cache_age_ms` to the candidates payload on `hit` to expose warm-cache freshness for diagnostics.
