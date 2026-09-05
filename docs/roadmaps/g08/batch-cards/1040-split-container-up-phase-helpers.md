# 1040 - Split Container Up Phase Helpers

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: ready after `1039`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Make `container up` orchestration easier to change safely by extracting phase
helpers without changing behavior.

## Work

- name the current bring-up phases
- extract validation/runtime-prep/compose/finalize/render helpers where they
  reduce mixed decisions
- preserve cleanup behavior at each failure point
- add or adjust focused tests around interrupt and cleanup paths

## Guardrails

- no backend behavior changes
- no gateway/DNS/secret/host-process behavior changes
- no output wording churn unless proven equivalent

## Acceptance

- `run_container_up` reads as phase orchestration
- failure cleanup behavior remains covered
- focused lifecycle tests pass

## Validation

- focused container lifecycle tests
- `effigy test --plan`

## Evidence

- [`../../../logs/archive/2026-06/04-212126-container-up-phase-boundary-cleanup.md`](../../../logs/archive/2026-06/04-212126-container-up-phase-boundary-cleanup.md)

## Next Task

Run [`1041-converge-repo-marker-rules.md`](./1041-converge-repo-marker-rules.md).
