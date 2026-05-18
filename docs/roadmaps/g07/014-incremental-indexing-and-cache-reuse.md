# g07.014 - Incremental Indexing And Cache Reuse

Status: Complete
Depends on: `g07.013`

## Goal

Make `effigy graph index` cheap when nothing changed and bounded when only a
small slice changed.

## Scope

- persist enough file-level index metadata to avoid full extractor reruns when
  content hash and extractor contract are unchanged
- distinguish cold, changed-slice, and true no-op index paths
- keep stale detection explicit and deterministic
- preserve full rebuild fallback when schema or extractor compatibility shifts

## Guardrails

- no silent stale reuse when extractor logic or schema changed
- no cross-repo cache bleed
- no cache contract that language extractors write directly
- no performance win that drops diagnostics or failed-path reporting

## Acceptance

- no-op indexing is materially faster than the `g07.012` baseline
- changed-slice indexing reruns only the required file set
- explicit rebuild invalidation exists for schema or extractor-version drift

## Next Task

Execute `935`.
