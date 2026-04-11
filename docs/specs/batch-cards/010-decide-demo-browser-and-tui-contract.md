# 010 Decide Demo Browser And TUI Contract

Status: ready
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Lock the first bounded browser contract for demos:

- sidebar/list expectations
- grouping and filter model
- status and gap badge model
- logs/receipts/artifact drilldown expectations

## In Scope

- define what the first TUI browser must be able to show and navigate
- define minimum grouping/filtering dimensions
- define the minimum runner data the browser depends on
- keep the contract compatible with the already-settled object, runner, and
  coverage semantics

## Out Of Scope

- TUI implementation details
- desktop-client decisions
- repo migrations
- project-specific UI polish

## Acceptance Criteria

- `g02.003` clearly states the first browser/TUI contract
- the browser contract depends on explicit runner and coverage data, not hidden
  inference
- the next batch can move to pilot reconciliation or implementation planning
  without reopening operator-browser basics

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch drifts into concrete layout/palette/widget implementation
- browser requirements become speculative beyond what the settled runner and
  coverage model can actually supply

## Next Task

Complete this planning batch, then leave the next move explicit as either pilot
reconciliation against Signal or the first bounded implementation-planning lane.
