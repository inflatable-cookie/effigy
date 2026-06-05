# 1038 - Define Command Surface Descriptor Seam

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: ready after `1037`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Introduce the smallest shared command metadata seam that reduces declaration
drift without rewriting parser or runner dispatch.

## Work

- add descriptor tests against current command/help metadata
- migrate one low-risk metadata consumer
- leave parser and runner arms explicit
- document deferred command-surface cleanup if any list remains manual

## Guardrails

- no CLI grammar changes
- no JSON schema changes
- no help text redesign
- no macro-only command framework

## Acceptance

- descriptor coverage test fails on missing command metadata
- selected metadata consumer reads from the descriptor seam
- released command behavior remains unchanged

## Evidence

- [`../../../logs/2026-06/04-210225-command-surface-descriptor-seam.md`](../../../logs/2026-06/04-210225-command-surface-descriptor-seam.md)

## Validation

- `cargo test -p effigy-cli`
- focused CLI output tests for released surfaces

## Next Task

Run `1039`.
