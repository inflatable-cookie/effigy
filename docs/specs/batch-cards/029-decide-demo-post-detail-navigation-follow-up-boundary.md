# 029 Decide Demo Post-Detail-Navigation Follow-Up Boundary

Status: ready
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Reassess the demo browser after detail-pane navigation shipped and choose the
next bounded operator-visible browser slice without widening into deeper
runtime or desktop-client work.

## In Scope

- review what the shipped self-hosted demos now expose in the browser
- identify the next tight operator-visible gap after list/detail, query,
  artifact, recent-output, and detail-navigation work
- choose the next bounded browser slice and explain why it is tighter than
  broader runtime or rendering work

## Out Of Scope

- terminal emulation
- generic runtime cancellation expansion
- multi-attempt history
- desktop-client foundation work

## Acceptance Criteria

- the next browser follow-up is explicitly chosen from live evidence rather
  than intuition
- the boundary stays inside bounded browser ergonomics
- broader runtime and client-surface questions remain explicitly deferred

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch turns into implementation instead of a boundary decision
- the batch reopens generic runtime handles or terminal behavior
- the batch depends on desktop-client assumptions to feel coherent

## Next Task

Choose the next bounded browser follow-up after detail-pane navigation, then
open the corresponding ready card so the lane has a clean continuation point.
