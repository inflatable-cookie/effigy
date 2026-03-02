# Completion Candidates CI Warm-Hit Age-Bound Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Strengthen warm-hit cache telemetry coverage by asserting `cache_age_ms` remains bounded by the effective TTL.

## Changes
- Updated `024-ci-and-automation-recipes.md` warm-hit snippet with:
  - `jq -e '.result.cache_age_ms <= .result.effective_cache_ttl_ms'`
- Strengthened completion candidates JSON contract test:
  - `builtin_completion_candidates_json_contract_reports_cache_hit_on_unchanged_rerun`
  - Now asserts hit `cache_age_ms` is numeric and `<= effective_cache_ttl_ms`.

## Validation
- command: `cargo test builtin_completion_candidates_json_contract_reports_cache_hit_on_unchanged_rerun -- --exact`
  - result: pass
- command: `./scripts/check-doc-links.sh`
  - result: pass

## Risks / Follow-ups
- The bound check is conservative (`<= ttl`) and does not enforce a minimum warm-hit age.

## Next
- Add a targeted test proving miss states always keep `cache_age_ms` and `cache_ttl_ms` null across policy paths.
