# Completion Candidates CI Telemetry Recipe Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add CI-ready recipes for asserting completion cache policy telemetry in JSON output.

## Changes
- Updated `024-ci-and-automation-recipes.md` with:
  - deterministic env override assertions (`cache_ttl_source=env`, `effective_cache_ttl_ms=750`)
  - invalid env fallback assertions (`cache_ttl_source=env_invalid`, `effective_cache_ttl_ms=2000`)

## Validation
- command: `./scripts/check-doc-links.sh`
  - result: pass

## Risks / Follow-ups
- Recipe assumes `jq` availability in CI runners (already required by other JSON contract tooling docs).

## Next
- Add one negative-case example that verifies `cache_ttl_ms` remains `null` on miss states in automation checks.
