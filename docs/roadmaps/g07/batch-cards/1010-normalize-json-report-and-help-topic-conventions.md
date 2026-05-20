# 1010 - Normalize JSON Report And Help Topic Conventions

Roadmap: [`../060-json-help-contract-consistency-cleanup.md`](../060-json-help-contract-consistency-cleanup.md)
Strict lane: [`../../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md`](../../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Reduce drift in command JSON/report rendering and repeated CLI help fragments.

## Work

- identify the smallest shared help fragments with repeated option/example
  blocks
- extract those fragments without making topic files opaque
- document or encode the preferred JSON rendering convention
- migrate only low-risk duplicated JSON wrappers
- add focused output stability tests where needed

## Guardrails

- no breaking JSON changes
- no broad release/distribution rewrite
- no macro-heavy help system
- no wording churn without a clear reason

## Acceptance

- duplicate help blocks are reduced
- JSON/report convention is clearer than before
- touched output remains stable under tests

## Next Task

Start [`1011-trim-runner-domain-and-test-fixture-duplication.md`](./1011-trim-runner-domain-and-test-fixture-duplication.md).
