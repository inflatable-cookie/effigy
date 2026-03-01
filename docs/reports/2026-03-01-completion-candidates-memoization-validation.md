# Completion Candidates Memoization Validation

Date: 2026-03-01
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add short-TTL memoization for `effigy completion candidates`.
- Invalidate cached candidates when catalog manifest mtimes change.
- Preserve deterministic candidate output while exposing cache diagnostics for JSON consumers.

## Changes
- Added in-process cache for completion candidate base set, keyed by repo root.
- Cache TTL set to 2 seconds.
- Cache entries are invalidated when any discovered manifest mtime stamp changes.
- `effigy.completion.candidates.v1` now includes:
  - `cache_hit` (boolean)
  - `manifest_count` (number of tracked manifests in the candidate scan)
- Integrated script rendering helpers via `src/runner/builtin/completion/scripts.rs`.

## Validation
- command: `cargo fmt`
  - result: pass
- command: `cargo test completion_ -- --test-threads=1`
  - result: pass
- command: `cargo test builtin_completion_candidates_ -- --test-threads=1`
  - result: pass
- command: `./scripts/check-json-contracts.sh --fast`
  - result: pass

## Evidence
- Miss then hit on unchanged rerun:
  - first run: `cache_hit=false`
  - immediate rerun: `cache_hit=true`
- Invalidation on manifest change:
  - after updating `effigy.toml` mtime/content: `cache_hit=false`
- Expiry on TTL:
  - after sleeping >2s without changes: `cache_hit=false`

## Risks / Follow-ups
- Cache is process-local; each new `effigy` invocation starts with empty memoization state.
- Extremely large workspaces may still see notable cold-scan latency.

## Next
- Add optional persistent on-disk candidate index with bounded size and safe versioning so repeated CLI invocations can reuse warm completion data.
