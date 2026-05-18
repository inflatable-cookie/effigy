# 904 - Implement First-Party Extractor Framework

Roadmap: [`../004-first-party-language-extractor-framework.md`](../004-first-party-language-extractor-framework.md)
Strict lane: [`../../../specs/085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Create the internal boundary that language extractors use.

## Scope

- add internal `LanguageIndexer` trait
- add `GraphSink`
- add shared source range/location types
- add extractor diagnostics
- isolate per-file extraction failures
- add version/freshness interaction for extractors
- add fake extractor test harness

## Guardrails

- no plugin runtime
- no dynamic libraries
- no process extractor protocol
- no storage writes from extractors

## Acceptance

- a fake extractor can emit validated graph facts
- extractor diagnostics appear in index output
- extractor version changes mark files stale
- new extractors do not touch CLI or DB internals

## Next Task

Execute `906`.
