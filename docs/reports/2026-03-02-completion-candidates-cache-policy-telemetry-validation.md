# Completion Candidates Cache Policy Telemetry Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Expose effective completion cache policy diagnostics in `effigy completion candidates --json`.
- Keep existing cache hit/miss telemetry behavior unchanged.

## Changes
- Added JSON fields to `effigy.completion.candidates.v1`:
  - `effective_cache_ttl_ms`
  - `cache_ttl_source` (`default` or `env`)
- Extended completion-cache TTL policy parser to return both effective TTL and source.
- Preserved existing behavior:
  - `cache_ttl_ms` remains hit-only
  - `cache_age_ms` remains hit-only

## Validation
- command: `cargo fmt`
  - result: pass
- command: `cargo test completion_candidates_cache_ttl_ -- --test-threads=1`
  - result: pass
- command: `cargo test completion_ -- --test-threads=1`
  - result: pass
- command: `./scripts/check-json-contracts.sh --fast`
  - result: pass

## Evidence
- Default completion run emits:
  - `effective_cache_ttl_ms=2000`
  - `cache_ttl_source=default`
- Hit response still includes `cache_ttl_ms` and `cache_age_ms`.
- Miss responses still keep `cache_ttl_ms=null` and `cache_age_ms=null`.

## Risks / Follow-ups
- `cache_ttl_source` currently reports only `default`/`env`; it does not distinguish invalid env fallback from unset env.

## Next
- Add a `cache_ttl_source=env_invalid` variant for malformed env overrides to make diagnostics explicit.
