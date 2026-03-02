# Completion Candidates JSON Examples Policy-Delta Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add side-by-side completion-candidates JSON examples that show cache telemetry differences between warm-hit and miss policy fallback.

## Changes
- Updated `026-json-payload-examples.md` completion-candidates section with:
  - warm-hit example (`cache_state=hit`, numeric `cache_age_ms`, non-null `cache_ttl_ms`)
  - miss env-invalid fallback example (`cache_state=miss_initial`, `cache_age_ms=null`, `cache_ttl_ms=null`, `cache_ttl_source=env_invalid`)

## Validation
- command: `./scripts/check-doc-links.sh`
  - result: pass

## Risks / Follow-ups
- Examples are illustrative snapshots; field values like `cache_age_ms` are runtime-dependent.

## Next
- Add a tiny docs QA check that verifies both completion-candidates example blocks contain required cache telemetry keys.
