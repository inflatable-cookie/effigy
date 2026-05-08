# 285 Plan Post-Transfer Data Lifecycle Batch

Status: archived
Updated: 2026-04-18
Roadmap: `g02.015`
Spec: `docs/specs/015-persistent-data-and-volume-lifecycle-strict-lane.md`

## Objective

Choose the next bounded `g02.015` batch now that generated-compose transfer is
real.

## Context

`280` made reset retention real. `282` made inventory real. `284` now makes
bounded transfer real on the generated-compose path. The remaining roadmap
surface is wider and more ambiguous:

- media bind-mount lifecycle
- task-owned seeding
- `pull_production` hook orchestration

The next step needs an explicit product boundary instead of free-continuing
into hook behavior.

## In Scope

- assess the next bounded widening step after landed transfer
- choose whether media lifecycle or hook/seeding orchestration comes next
- update the strict-lane front door so one explicit execution card is ready

## Out Of Scope

- execution work
- cross-project data inventory or transfer
- backup scheduling
- broad multi-surface orchestration in one batch

## Acceptance

- one explicit next execution card exists for `g02.015`
- the chosen batch builds directly on the landed reset, inventory, and transfer
  surfaces
- the front-door planning surfaces stop pointing at already-landed `284`

## Result

The next bounded widening step is media bind-mount lifecycle on the
generated-compose path.

Why this comes next:

- it stays on the core data-lifecycle contract instead of widening into
  orchestration early
- it builds directly on the landed reset, inventory, and transfer surfaces
- it keeps Effigy on honest generated-compose ownership before any task-owned
  seeding or `pull_production` hook behavior

Execution is now handed to [`286-implement-media-bind-mount-lifecycle-foundation.md`](./286-implement-media-bind-mount-lifecycle-foundation.md).

## Next Task

Execute `286` to make generated-compose media bind-mount lifecycle declarations
real, then stop in planning for the next bounded `g02.015` widening step.
