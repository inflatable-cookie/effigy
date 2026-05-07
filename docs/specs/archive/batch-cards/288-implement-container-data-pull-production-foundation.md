# 288 Implement Container Data Pull Production Foundation

Status: landed
Updated: 2026-04-18
Roadmap: `g02.015`
Spec: `docs/specs/015-persistent-data-and-volume-lifecycle-strict-lane.md`

## Objective

Make the next bounded `g02.015` orchestration surface real by adding generated-compose
`[containers.<name>.data].pull_production` hook ownership on the container path.

## Context

`280`, `282`, `284`, and `286` now cover reset retention, inventory, transfer,
and media lifecycle on the generated-compose path. The remaining roadmap work
has moved past core lifecycle primitives into orchestration.

Task-owned seeding should remain task-based. It already fits the shipped
workspace binding and Rhai/exec surfaces better than a new data-specific
abstraction. The next product gap that still needs real ownership is bounded
production-pull orchestration.

## In Scope

- add bounded `[containers.<name>.data].pull_production` manifest support
- define one honest product entrypoint for invoking that hook on the generated-compose path
- wire hook execution through existing task/Rhai/script execution surfaces without inventing a
  second seeding abstraction
- keep direct `compose_file` ownership explicit and out of scope
- add focused coverage for manifest parsing, command dispatch, and bounded hook execution

## Out Of Scope

- new seed-specific manifest abstractions
- direct `compose_file` pull orchestration
- broad migration-bundle frameworks
- real-project proof
- multi-container orchestration beyond one bounded hook contract

## Acceptance

- generated-compose containers can declare one bounded `pull_production` hook through a
  manifest-owned data surface
- the product exposes one honest invocation path for that hook
- hook execution reuses existing task/script primitives instead of inventing a second seeding
  system
- focused tests cover manifest, runner, and reporting behavior

## Result

Generated-compose containers can now declare
`[containers.<name>.data].pull_production` as either a repo-relative shell
script path or `rhai:` script path.

What landed:

- manifest-owned `data.pull_production` parsing on the container path
- `effigy container data pull-production` as the bounded product entrypoint
- generated-compose ownership checks plus direct `compose_file` rejection on
  this path
- precondition ownership that brings the environment to ready state before the
  hook runs
- hook execution through existing script primitives instead of a second
  seeding abstraction
- focused coverage for parsing, report shaping, runner rejection, and CLI
  execution

The lane now stops in planning again before deciding whether one more proof or
closeout batch is needed.

## Next Task

Execute [`289-plan-post-pull-production-lane-closeout.md`](./289-plan-post-pull-production-lane-closeout.md)
to choose the next explicit `g02.015` step after pull-production foundation.
