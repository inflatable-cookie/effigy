# Completion Candidates Cache Policy Invalid-Env Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Extend completion cache policy telemetry to distinguish invalid env TTL values.

## Changes
- `cache_ttl_source` now reports:
  - `default` when env override is unset
  - `env` when env override parses successfully
  - `env_invalid` when env override is present but malformed
- Invalid env values continue to fall back to default effective TTL (`2000` ms).

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
- parser test (`not-a-number`) returns:
  - `ttl_ms=2000`
  - `source=env_invalid`
- default and valid-env branches remain unchanged in parser tests.

## Risks / Follow-ups
- Completion candidates contract tests currently assert default-path telemetry only; add env override integration assertions if explicit env test harness is introduced.

## Next
- Add a targeted JSON contract test helper for scoped env overrides so `cache_ttl_source=env` and `env_invalid` can be asserted end-to-end.
