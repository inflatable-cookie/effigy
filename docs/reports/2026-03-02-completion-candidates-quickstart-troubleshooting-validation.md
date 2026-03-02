# Completion Candidates Quickstart Troubleshooting Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add an operator-facing troubleshooting recipe for completion candidate cache telemetry in quick start docs.

## Changes
- Updated `021-quick-start-and-command-cookbook.md` with a JSON troubleshooting snippet for:
  - `cache_state`
  - `cache_ttl_source`
  - `effective_cache_ttl_ms`
  - hit-only cache freshness fields (`cache_age_ms`, `cache_ttl_ms`)

## Validation
- command: `./scripts/check-doc-links.sh`
  - result: pass

## Risks / Follow-ups
- Recipe assumes operators inspect JSON output directly; a dedicated troubleshooting guide section could add copy-paste parsing helpers later.

## Next
- Add a compact CI/automation snippet to `024-ci-and-automation-recipes.md` showing assertion of `cache_ttl_source` for deterministic environments.
