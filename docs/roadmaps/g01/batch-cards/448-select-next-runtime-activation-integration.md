# 448 - Select Next Runtime Activation Integration

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next runtime activation integration point after `effigy exec`.

## Scope

- compare remaining activation callers:
  - standard task routed activation
  - DB seed activation
  - deferral activation
  - workspace seeded sessions
- decide whether to move a second simple caller or start standard task
  activation
- create the next bounded implementation card
- keep side effects unchanged

## Non-Goals

- no side-effect migration
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the next runtime activation integration card is
ready and the lane/front-door docs point to it.

## Decision

Use DB seed activation as the next integration point.

Why:

- it has one activation call in `prepare_db_seed_runtime`
- it already resolves an effective policy before activation
- it uses a distinct runtime session policy through
  `data_seed_runtime_session_context`
- it is directly relevant to the path/container hardening issues that motivated
  this modularisation work

Do not start standard task activation yet. It remains the main target, but it
mixes routing, auto-up retry, workspace-seeded shell handoff, inline cleanup,
and direct container execution. DB seed gives the runtime plan a second caller
without widening the side-effect migration.

## Closeout

Selected DB seed activation and created card `449`.

## Validation

- docs path check for updated spec and roadmap front doors
- `git diff --check`

## Next Task

Start card
[`449-wire-runtime-activation-plan-into-db-seed.md`](./449-wire-runtime-activation-plan-into-db-seed.md).
