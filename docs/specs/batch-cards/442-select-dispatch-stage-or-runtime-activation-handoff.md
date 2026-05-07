# 442 - Select Dispatch Stage or Runtime Activation Handoff

Lane: [`044-execution-pipeline-ownership-strict-lane.md`](../044-execution-pipeline-ownership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Decide whether `g04.002` needs one more execution dispatch-stage input before
handing off to `g04.003` runtime activation planning.

## Scope

- review the current execution planning summaries:
  - dispatch
  - preflight
  - discovery
  - selection
  - binding
- inspect standard and managed pipeline ownership after the new summaries
- decide whether another pure execution plan materially shrinks runner logic
- decide whether runtime activation should now take over as the next roadmap
- create the next bounded implementation or closeout card

## Non-Goals

- no runtime activation implementation
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the next ready card is explicit: either another
`g04.002` implementation slice or a `g04.002` closeout/`g04.003` handoff.

## Decision

Hand off to runtime activation after a `g04.002` closeout card.

Another pure execution dispatch-stage input would not materially shrink the
runner now. The remaining size and ownership pressure in
`standard.rs` and `managed.rs` is mostly runtime/container behavior:

- container policy loading
- runtime activation
- workspace-seeded sessions
- inline workspace activation and cleanup
- direct compose calls
- direct docker capture calls
- managed gateway/session handling

Those belong to `g04.003` runtime activation and later container operation
work, not another execution-planning summary. `g04.002` has created the typed
front half of the execution pipeline:

- `ExecutionDispatchPlan`
- `ExecutionPreflightInput`
- `ExecutionRuntimeArgsPlan`
- `ExecutionDiscoveryPlan`
- `ExecutionSelectionPlan`
- `ExecutionBindingPlan`

The `g04.002` closeout should document the remaining runner-owned side effects
and then hand the lane to `g04.003`.

## Closeout

Selected closeout and runtime activation handoff as the next step. Created card
`443`.

## Validation

- docs path check for updated spec and roadmap front doors
- `git diff --check`

## Next Task

Start card
[`443-close-execution-pipeline-ownership-and-handoff-runtime-activation.md`](./443-close-execution-pipeline-ownership-and-handoff-runtime-activation.md).
