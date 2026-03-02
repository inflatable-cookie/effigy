# Completion Candidates CI Warm-Hit TTL Consistency Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add a CI recipe that forces a completion cache hit and validates TTL consistency fields.

## Changes
- Updated `024-ci-and-automation-recipes.md` with a warm-hit check that:
  - runs completion candidates twice
  - asserts second response is `cache_state=hit`
  - asserts `cache_ttl_ms` is non-null on hit
  - asserts `cache_ttl_ms == effective_cache_ttl_ms`

## Validation
- command: `./scripts/check-doc-links.sh`
  - result: pass

## Risks / Follow-ups
- The warm-hit check assumes both invocations occur within TTL; very slow runners may require a higher configured TTL for deterministic behavior.

## Next
- Add an optional defensive assertion that `cache_age_ms` is numeric on warm-hit responses.
