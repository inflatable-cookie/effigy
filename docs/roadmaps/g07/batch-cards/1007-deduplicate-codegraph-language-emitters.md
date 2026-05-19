# 1007 - Deduplicate Codegraph Language Emitters

Roadmap: [`../057-codegraph-language-emitter-deduplication.md`](../057-codegraph-language-emitter-deduplication.md)
Strict lane: [`../../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md`](../../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-19

## Purpose

Remove duplicated graph-record emission code from language extractors without
changing extractor semantics.

## Work

- add a shared internal emitter helper module
- migrate JS/PHP duplicated parse diagnostic, symbol, contains-edge, and
  unresolved-edge helpers
- migrate Python only where the helper fits cleanly
- keep language-specific traversal and symbol decisions local
- add or preserve focused extractor tests
- rerun duplicate scan for the touched files

## Guardrails

- no graph schema changes
- no query ranking changes
- no language coverage expansion
- no generic language framework

## Acceptance

- critical duplicate blocks across JS/PHP/Python emitter boilerplate are gone
- extractor output remains stable under existing tests
- helper API is narrow and graph-record-specific

## Next Task

Start [`1008-decompose-codegraph-manifest-and-query-modules.md`](./1008-decompose-codegraph-manifest-and-query-modules.md).
