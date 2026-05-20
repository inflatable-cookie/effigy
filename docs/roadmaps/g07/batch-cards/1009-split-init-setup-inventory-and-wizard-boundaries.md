# 1009 - Split Init Setup Inventory And Wizard Boundaries

Roadmap: [`../059-init-setup-module-boundary-cleanup.md`](../059-init-setup-module-boundary-cleanup.md)
Strict lane: [`../../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md`](../../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Make the init setup code easier to reason about by separating model, detection,
rendering, command construction, execution, and test support.

## Work

- split `init/inventory.rs` into local setup modules
- keep wizard prompt flow separate from action execution
- introduce compact fake port/test support if it reduces test noise
- preserve checklist and action execution JSON contracts
- rerun init-focused tests and help rendering tests

## Guardrails

- no new setup jobs
- no second onboarding command
- no release/deploy/state/distribution mutation
- no prompt behavior in non-TTY contexts

## Acceptance

- init setup ownership is clear from module names
- current init wizard/checklist behavior is preserved
- tests read as behavior proof rather than plumbing proof

## Next Task

Start [`1010-normalize-json-report-and-help-topic-conventions.md`](./1010-normalize-json-report-and-help-topic-conventions.md).
