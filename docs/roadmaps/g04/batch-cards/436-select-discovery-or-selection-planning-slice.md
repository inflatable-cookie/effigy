# 436 - Select Discovery or Selection Planning Slice

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next `g04.002` implementation slice after preflight input moved
behind the dispatch plan.

## Scope

- review the current split between `effigy-execution` and runner preflight
- decide whether to move discovery output shape or selection input planning next
- identify manifest/catalog lifetime blockers before moving code
- create the next smallest implementation card

## Non-Goals

- no runtime activation migration
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the next bounded implementation card is ready and
the lane/front-door docs point to it.

## Decision

Move discovery output shape next.

Selection still carries borrowed `LoadedCatalog` and `TaskSelection` values, so
moving it now would pull manifest lifetimes and routing selection behavior into
the new crate before the preflight/discovery boundary is typed enough. That is
too wide for the next card.

Discovery has a cleaner split:

- `effigy-execution` can own the request/plan/report shape
- runner can still perform side-effectful context resolution and catalog
  discovery
- selector parsing can move behind the shared planning surface because
  `effigy-execution` already depends on `effigy-tasks`
- loaded catalogs can stay runner-owned until a later selection slice

The next card should not move `discover_catalogs_allow_missing` or
`resolve_command_context_from_cwd` out of runner. It should make their output
land in an `effigy-execution` discovery plan type, then keep the current
`ExecutionPreflight` struct as the runner-local aggregate for now.

## Next Implementation Slice

Card `437` should add discovery request/output plan types to
`effigy-execution` and make runner preflight build from them.

The first useful split is:

- `ExecutionDiscoveryInput`
- `ExecutionDiscoveryPlan`
- `ExecutionCatalogDiscoveryPlan` or an equivalent runner-owned catalog
  handoff shape only if it stays lifetime-light
- a selector parser helper that returns a typed selector plan

Expected follow-on after `437`: decide whether selection can consume a
lifetime-light `ExecutionSelectionInput`, or whether one more catalog handoff
shape is needed first.

## Validation

- docs path check for updated spec and roadmap front doors
- `git diff --check`

## Closeout

Selected discovery output shape as the next implementation slice and created
card `437`.

## Next Task

Start card
[`437-add-execution-discovery-plan-foundation.md`](./437-add-execution-discovery-plan-foundation.md).
