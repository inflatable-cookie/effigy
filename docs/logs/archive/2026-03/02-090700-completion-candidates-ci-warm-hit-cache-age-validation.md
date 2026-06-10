# Completion Candidates CI Warm-Hit Cache-Age Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Extend warm-hit CI recipe coverage to assert `cache_age_ms` shape on hit responses.

## Changes
- Updated `024-ci-and-automation-recipes.md` warm-hit snippet with:
  - `jq -e '(.result.cache_age_ms | type) == "number"'`

## Validation
- command: `./scripts/check-doc-links.sh`
  - result: pass

## Risks / Follow-ups
- Numeric type assertion validates shape, not strict age bounds.

## Next
- Add an optional guard that `cache_age_ms >= 0` and `cache_age_ms < effective_cache_ttl_ms` when runners are stable enough for bounded timing checks.
