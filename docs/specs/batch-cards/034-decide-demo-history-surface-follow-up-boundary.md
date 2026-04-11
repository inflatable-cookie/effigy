# 034 Decide Demo History Surface Follow-Up Boundary

Status: ready
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose where demo history should widen next now that Effigy ships bounded
runner-side attempt history through `demo inspect`.

## In Scope

- assess whether the next bounded history value belongs in `demo list`, the
  browser, or a separate result-timeline query surface
- use the self-hosted demo inventory and shipped browser baseline as the
  decision anchor rather than abstract UI preference
- keep the next slice centered on history visibility and result review

## Out Of Scope

- implementing browser timelines or list summaries in this decision batch
- multi-attempt concurrency, queueing, or generic runtime cancellation
- desktop-client planning or repo migration work

## Acceptance Criteria

- the next bounded history slice is explicitly chosen and justified from the
  shipped runner/browser evidence
- the decision leaves the lane with one clean next card instead of reopening
  broad browser churn
- broader runtime and desktop scope remain deferred

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch turns into implementation instead of a boundary decision
- the choice is made from UI taste instead of the current runner/browser
  evidence
- the batch widens into generic timeline or desktop planning

## Next Task

Execute the active decision batch to choose whether the next bounded
demo-history slice belongs in `demo list`, the browser, or a separate
result-timeline query surface.
