# g07.019 - Safe Scan Metadata Reuse

Status: Complete
Depends on: `g07.017`

## Goal

Reduce repeated scan work without weakening added/changed/deleted path
correctness.

## Scope

- reuse scan metadata where the current command already proved freshness
- reduce duplicate repo walks inside `graph index`, `status`, and stale
  reporting
- prefer local command-bounded reuse over persistent speculative caches
- keep extractor invalidation and file-content checks explicit

## Guardrails

- no watcher-backed design
- no silent carry-forward of stale path sets
- no “fast path” that can miss new files or deleted files
- no DB schema churn unless the value is clearly proven

## Acceptance

- no-op index and stale/status paths get measurably cheaper
- regression coverage proves new, changed, and deleted file detection still
  works

## Next Task

No active task remains in this roadmap.
