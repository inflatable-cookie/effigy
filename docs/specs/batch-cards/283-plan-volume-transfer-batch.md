# 283 Plan Volume Transfer Batch

Status: landed
Updated: 2026-04-18
Roadmap: `g02.015`
Spec: `docs/specs/015-persistent-data-and-volume-lifecycle-strict-lane.md`

## Objective

Choose the next bounded `g02.015` batch now that `effigy container data list`
is real.

## Context

`280` made retention real and `282` made inventory real. Operators can now see
what Effigy-managed data volumes exist before wiping a stack, but they still
cannot move that data between machines or preserve it outside the local Docker
runtime.

The roadmap still promises wider lifecycle work:

- `effigy container data export <volume> <path>`
- `effigy container data import <volume> <path>`
- task-owned seeding and `pull_production` hooks

The next step should stay on the narrowest trustworthy boundary instead of
jumping into hook orchestration before transfer primitives exist.

## In Scope

- assess the next bounded `g02.015` widening step after `282`
- choose whether export/import should come next, and if so on what boundary
- update the strict-lane front door so the next execution card is explicit

## Out Of Scope

- execution work
- media bind-mount lifecycle
- seeding orchestration
- `pull_production` hooks
- cross-project data inventory

## Acceptance

- one explicit next execution card exists for `g02.015`
- the chosen batch builds directly on the landed reset and inventory surfaces
- the front-door planning surfaces stop pointing at already-landed `282`

## Result

This planning batch is now landed.

Decision:

- the next bounded `g02.015` widening step should be managed volume transfer,
  not seeding hooks or `pull_production`

Why this batch comes next:

- reset plus inventory now cover retention and visibility, but not portability
- export/import uses already-shipped command substrate in
  `effigy-catalog::volumes`
- hook orchestration widens lifecycle semantics before the basic transfer
  primitive is even real
- keeping transfer on the generated-compose path preserves the same
  trustworthy ownership boundary used by `reset --keep-data` and `data list`

The next explicit execution batch is now card `284`.

## Next Task

Execute `284` to land bounded generated-compose `container data export/import`
transfer.
