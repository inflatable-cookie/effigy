# 009 Decide Demo Coverage And Gap Model

Status: ready
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Lock the first bounded coverage model for demos:

- how proof presence is expressed
- how missing or planned proof is represented
- how stale or broken proof is surfaced
- what minimum browser-facing gap visibility the model must support

## In Scope

- define coverage classes and gap classes
- define the relationship between demo status and proof coverage
- define what metadata the browser needs to show “exists / missing / broken /
  stale”
- keep the result compatible with the already-settled demo registry and runner
  contract

## Out Of Scope

- TUI layout
- desktop-client decisions
- repo migrations
- detailed receipt rendering or artifact viewers

## Acceptance Criteria

- `g02.003` clearly states how proof gaps are modeled
- the browser contract can rely on explicit gap visibility rather than
  inference from whatever demos happen to exist
- the next batch can move to browser/TUI contract shaping without reopening
  proof coverage semantics

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch turns into a full project-planning taxonomy instead of a bounded
  verification coverage model
- gap modeling starts depending on project-specific nomenclature rather than a
  reusable Effigy contract

## Next Task

Complete this planning batch, then leave the next move explicit as either the
browser/TUI contract or pilot reconciliation against Signal.
