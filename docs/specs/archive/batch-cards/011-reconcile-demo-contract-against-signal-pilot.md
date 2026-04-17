# 011 Reconcile Demo Contract Against Signal Pilot

Status: complete
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Pressure-test the settled demo object, runner, coverage, and browser contract
against Signal's existing demo proof surface.

## In Scope

- compare the current contract to Signal's manifest/scenario/receipt/view shape
- identify which parts of Signal map cleanly to the Effigy model
- identify which parts are orchestration debt or project-specific layering
- leave a bounded implementation-planning move that does not reopen the model

## Out Of Scope

- implementation work in Effigy
- migrating Signal in this batch
- TUI widget/layout detail
- desktop-client decisions

## Acceptance Criteria

- `g02.003` includes a concrete reconciliation between the contract and Signal
- the repo records which Signal concepts become first-class runner/data surface
- the repo records which Signal concepts remain pilot-specific or harness debt
- the next card can move into implementation planning without re-litigating
  the model

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch starts redesigning the already-settled demo object or browser
  contract without new evidence
- the batch drifts into code implementation instead of pilot reconciliation

## Next Task

Use this reconciliation result to open the first bounded implementation-planning
card for the demo runner foundation.
