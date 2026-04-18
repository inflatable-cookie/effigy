# 281 Plan Volume Inventory Batch

Status: landed
Updated: 2026-04-18
Roadmap: `g02.015`
Spec: `docs/specs/015-persistent-data-and-volume-lifecycle-strict-lane.md`

## Objective

Choose the next bounded `g02.015` execution batch now that generated-compose
`reset --keep-data` is real.

## Context

`280` landed the first lifecycle surface: reset can now preserve persistent
named volumes. The next product gap is visibility. Operators still cannot ask
Effigy what managed data volumes exist before deciding whether to reset,
export, or import anything.

The substrate already exists:

- generated-compose policy now carries managed-volume metadata
- `effigy-catalog::volumes` already has list/inspect command specs
- the container CLI already owns the repo-local operator surface

## In Scope

- assess the next `g02.015` widening step after `280`
- choose the smallest trustworthy next batch
- update the strict-lane and front-door planning surfaces

## Out Of Scope

- execution work
- volume export/import
- media bind-mount lifecycle
- `pull_production` hooks or seeding orchestration

## Acceptance

- one explicit next execution card exists for `g02.015`
- the chosen batch builds directly on the now-shipped reset foundation
- the front-door planning surfaces stop leaving the next `g02.015` move vague

## Result

The next bounded `g02.015` batch should be volume inventory, not transfer or
hook orchestration.

Why this batch comes next:

- `reset --keep-data` made retention real, but operators still lack a product
  view of what data Effigy owns
- `container data list` uses the already-shipped volume command substrate
  directly and keeps the next widening read-only
- export/import and production-pull hooks widen mutation and orchestration
  before the basic inventory surface is even real

The next explicit execution batch is now card `282`.

## Next Task

Execute `282` to land bounded `effigy container data list` inventory.
