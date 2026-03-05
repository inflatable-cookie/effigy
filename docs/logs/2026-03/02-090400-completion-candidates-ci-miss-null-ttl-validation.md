# Completion Candidates CI Miss Null-TTL Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add a CI recipe for asserting miss-state nullability of hit-only completion cache TTL field.

## Changes
- Updated `024-ci-and-automation-recipes.md` with a miss-path assertion snippet:
  - `cache_state != "hit"`
  - `cache_ttl_ms == null`

## Validation
- command: `./scripts/check-doc-links.sh`
  - result: pass

## Risks / Follow-ups
- Miss detection in CI is state/timing dependent; recipe intentionally validates nullability only when state is miss.

## Next
- Add an optional warm-run variant that intentionally produces a hit before asserting non-null `cache_ttl_ms`.
