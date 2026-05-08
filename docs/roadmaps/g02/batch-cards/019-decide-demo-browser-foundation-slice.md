# 019 Decide Demo Browser Foundation Slice

Status: archived
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Decide the first bounded implementation slice for a real demo browser/TUI on
top of the shipped registry, run lifecycle, and browser-facing query/state
surface.

## In Scope

- lock the minimum browser foundation scope that is honest to build next
- decide what state/query work is already sufficient from the CLI layer
- define what the first interactive client must consume from the shipped runner
- keep the slice small enough that it does not blur into broad runtime
  expansion or project-specific proof UI

## Out Of Scope

- implementing the TUI/browser itself
- broadening generic runtime cancellation
- multi-attempt history or queueing
- consumer-repo migration work

## Acceptance Criteria

- the next demo batch after `018` is explicit and bounded
- the first browser/TUI slice is grounded in the now-shipped query/state
  surface rather than re-opening model questions
- broader stoppability remains deferred unless a specific browser requirement
  proves it is now necessary

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch starts implementing UI code
- the batch re-opens already settled demo model or lifecycle decisions
- the batch turns into generic runtime cancellation planning

## Outcome

- the first honest browser slice is now fixed as a read-mostly list/detail TUI
  browser with in-browser `run`, `stop`, `rerun`, and refresh actions
- the self-hosted demos proved that the shipped CLI query/state layer is
  already sufficient for the browser foundation; the real missing capability is
  single-surface interaction, not more list/query semantics
- broader stoppability, generic runtime handles, live terminal streaming, and
  rich artifact rendering remain explicitly deferred

## Next Task

Implement the bounded browser foundation slice next through
`020-implement-demo-browser-list-detail-foundation.md`.
