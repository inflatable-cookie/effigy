# Completion Candidates CI Miss Hit-Only Nullability Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Ensure miss-state checks consistently enforce nullability for all hit-only telemetry fields.

## Changes
- Updated `024-ci-and-automation-recipes.md` miss-path snippet with:
  - `jq -e '.result.cache_age_ms == null'`
- Strengthened completion candidates JSON contract coverage:
  - `builtin_completion_candidates_json_contract_reports_env_ttl_policy` now asserts miss-initial `cache_age_ms` is null.
  - `builtin_completion_candidates_json_contract_reports_invalid_env_ttl_policy` now asserts miss-initial `cache_age_ms` is null.

## Validation
- command: `cargo test runner::json_contract_tests::builtin_completion_candidates_json_contract_reports_env_ttl_policy -- --exact`
  - result: pass
- command: `cargo test runner::json_contract_tests::builtin_completion_candidates_json_contract_reports_invalid_env_ttl_policy -- --exact`
  - result: pass
- command: `./scripts/check-doc-links.sh`
  - result: pass

## Risks / Follow-ups
- This adds coverage for miss-initial policy paths; miss-ttl and miss-manifest-change nullability remain covered by dedicated tests.

## Next
- Add a consolidated cache telemetry fixture under `docs/guides/026-json-payload-examples.md` showing hit vs miss policy-path deltas side-by-side.
