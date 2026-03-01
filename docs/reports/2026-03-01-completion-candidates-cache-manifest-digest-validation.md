# Completion Candidates Cache Manifest Digest Validation

Date: 2026-03-01
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Harden completion-candidate cache invalidation for manifest changes by using content-aware manifest stamps.
- Ensure invalidation still happens when manifest content changes but modified time is preserved.

## Changes
- Extended manifest stamp identity in completion candidate cache to include:
  - modified timestamp (nanoseconds since epoch, when available)
  - file length
  - deterministic FNV-1a 64-bit content digest
- Updated JSON contract coverage to validate `miss_manifest_change` when manifest content changes and mtime is restored.

## Validation
- command: `cargo fmt`
  - result: pass
- command: `cargo test builtin_completion_candidates_json_contract_invalidates_cache_on_manifest_change_with_preserved_mtime -- --test-threads=1`
  - result: pass
- command: `cargo test completion_ -- --test-threads=1`
  - result: pass
- command: `./scripts/check-json-contracts.sh --fast`
  - result: pass

## Evidence
- Initial candidates call: `cache_state=miss_initial`
- Second call after manifest task additions with restored mtime: `cache_state=miss_manifest_change`
- Updated selector set includes newly added task (`deploy`), confirming recomputation instead of stale hit reuse.

## Risks / Follow-ups
- Completion-candidate cache remains process-local and short-TTL; no cross-process persistence is implied.
- Digesting full manifest contents adds small per-manifest IO/CPU cost on validation checks.

## Next
- Add a bounded configuration hook for completion cache TTL so high-churn repos can tune staleness vs scan cost.
