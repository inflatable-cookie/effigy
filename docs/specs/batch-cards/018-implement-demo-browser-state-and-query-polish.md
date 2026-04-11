# 018 Implement Demo Browser State And Query Polish

Status: ready
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Make the shipped demo runner surface easier for a future TUI/browser and
operators to consume without starting UI implementation.

## In Scope

- tighten `demo list` / `demo inspect` around the browser row and drilldown
  contract
- add the minimum query surface needed for browser-style grouping or focused
  inspection without re-scanning the whole registry mentally
- keep active state, base status, gap class, freshness, receipt summary, and
  artifact references explicit in runner output

## Out Of Scope

- generic task-backed cancellation
- widening stop support beyond directly runner-owned attempts
- starting TUI/browser implementation
- multi-attempt history or queueing

## Acceptance Criteria

- the next execution slice is explicitly browser-state/query polish rather than
  blurred with runtime cancellation work
- runner output exposes the minimum browser row and drilldown data cleanly
- any new query flags stay bounded to discovery/inspection rather than
  inventing UI behavior in the CLI

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Stop Conditions

- the batch starts implementing a TUI
- the batch starts promising generic task-backed cancellation
- the batch broadens into attempt history or queue management

## Next Task

If browser-state polish lands cleanly, open the next bounded planning card for
broader stoppability/runtime handles; otherwise return the lane to a runtime
boundary checkpoint.
