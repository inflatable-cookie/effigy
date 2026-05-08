# 287 Plan Post-Media Data Orchestration Batch

Status: archived
Updated: 2026-04-18
Roadmap: `g02.015`
Spec: `docs/specs/015-persistent-data-and-volume-lifecycle-strict-lane.md`

## Objective

Choose the next bounded `g02.015` batch now that generated-compose media
bind-mount lifecycle is real.

## Context

`280`, `282`, `284`, and `286` now cover reset retention, inventory, transfer,
and media bind mounts on the generated-compose path. The remaining roadmap
surface moves from core lifecycle into orchestration:

- task-owned seeding
- `pull_production` hook orchestration
- real-project proof once the next bounded orchestration surface is explicit

The next step needs another product boundary instead of free-continuing into
hooks.

## In Scope

- assess the next bounded widening step after landed media lifecycle
- choose whether task-owned seeding or `pull_production` hook orchestration
  comes next
- update the strict-lane front door so one explicit execution card is ready

## Out Of Scope

- execution work
- direct `compose_file` ownership widening
- real-project proof
- broad multi-surface orchestration in one batch

## Acceptance

- one explicit next execution card exists for `g02.015`
- the chosen batch builds directly on the landed reset, inventory, transfer,
  and media lifecycle surfaces
- the front-door planning surfaces stop pointing at already-landed `286`

## Result

The next bounded widening step is generated-compose
`[containers.<name>.data].pull_production` hook orchestration.

Why this comes next:

- task-owned seeding already fits the shipped task, workspace binding, exec,
  and Rhai surfaces better than a new data-specific abstraction
- opening a seed-specific execution batch now would widen product semantics
  before proving the remaining manifest-owned orchestration contract
- `pull_production` still needs real product ownership, entrypoint, and
  boundary enforcement on the generated-compose path

Execution is now handed to
[`288-implement-container-data-pull-production-foundation.md`](./288-implement-container-data-pull-production-foundation.md).

## Next Task

`288` made generated-compose `data.pull_production` hook ownership real.
Continue from `289` for the next explicit `g02.015` planning step.
