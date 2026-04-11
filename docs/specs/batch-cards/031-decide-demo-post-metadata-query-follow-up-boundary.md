# 031 Decide Demo Post-Metadata-Query Follow-Up Boundary

Status: ready
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Reassess the demo browser after metadata-query parity shipped and choose the
next bounded operator-visible browser slice without widening into deeper
runtime or desktop-client work.

## In Scope

- review the shipped browser against the self-hosted demos now that metadata
  query parity is in place
- identify the next tight operator-visible gap from the fuller browser surface
- choose one bounded browser follow-up and explain why it is tighter than
  deeper runtime, rendering, or client-surface work

## Out Of Scope

- generic runtime cancellation expansion
- terminal emulation or richer log streaming
- desktop-client foundation work

## Acceptance Criteria

- the next browser follow-up is explicitly chosen from the now-shipped browser
  baseline rather than inferred from older partial slices
- the boundary stays inside bounded browser ergonomics
- broader runtime and client-surface questions remain explicitly deferred

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch turns into implementation instead of a boundary decision
- the batch reopens generic runtime handles or desktop-client assumptions
- the batch widens into richer log streaming or rendering without an explicit
  boundary decision first

## Next Task

Choose the next bounded browser follow-up after metadata-query parity, then
open the corresponding ready card so the lane has a clean continuation point.
