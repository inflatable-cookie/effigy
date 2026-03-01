# Completion Candidates Cache TTL Override Validation

Date: 2026-03-01
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add bounded environment override support for completion-candidate cache TTL.
- Keep default behavior unchanged when no override is set.

## Changes
- Added `EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS` support for completion cache expiry.
- Enforced bounded range for effective TTL: `100..60000` milliseconds.
- Preserved default TTL at `2000` milliseconds when env is missing or invalid.
- Added unit tests for default, valid override, low/high clamp, and invalid fallback behavior.

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
- Parsing behavior:
  - unset -> `2000`
  - `750` -> `750`
  - `5` -> `100` (clamped)
  - `999999` -> `60000` (clamped)
  - invalid string -> `2000`
- Existing completion JSON contract remains stable, with `cache_ttl_ms` still present on hits.

## Risks / Follow-ups
- TTL override remains process-local; there is no persisted repo-level cache policy yet.
- Very low TTL values (clamped at 100ms) can still increase refresh churn in high-frequency completion calls.

## Next
- Surface effective completion cache config in `effigy completion candidates --json` diagnostics for easier operator introspection.
