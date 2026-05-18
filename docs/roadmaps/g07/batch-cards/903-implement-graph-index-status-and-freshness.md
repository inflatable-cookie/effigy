# 903 - Implement Graph Index, Status, And Freshness

Roadmap: [`../003-graph-index-command-and-freshness-model.md`](../003-graph-index-command-and-freshness-model.md)
Strict lane: [`../../../specs/085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Add the first usable graph commands: index and status.

## Scope

- add `effigy graph index`
- add `effigy graph status --json`
- implement gitignore-aware repo walking
- exclude generated/cache/vendor paths by default
- track content hashes, mtimes, and extractor versions
- report stale/new/deleted/skipped/failed files
- make repeated no-op indexing idempotent

## Guardrails

- no silent query-time rebuilds
- no indexing `target`, `node_modules`, `vendor`, `.git`, or `.effigy/runtime`
  by default
- no deep extractor work beyond placeholder/fake extractor plumbing

## Acceptance

- `graph index` creates local artifacts
- `graph status --json` reports freshness and counts
- stale state is deterministic
- repeated indexing has stable output

## Next Task

Execute `906`.
