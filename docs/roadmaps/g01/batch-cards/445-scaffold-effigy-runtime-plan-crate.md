# 445 - Scaffold effigy-runtime-plan Crate

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Add the first dependency-light `effigy-runtime-plan` crate with pure runtime
activation request, plan, stage, and report types.

## Scope

- add `crates/effigy-runtime-plan`
- add workspace membership
- define:
  - `RuntimeActivationRequest`
  - `RuntimeActivationPlan`
  - `RuntimeActivationStage`
  - `RuntimeActivationRoute`
  - `RuntimeReadinessPlan`
  - `RuntimeAliasPlan`
  - `RuntimeLeasePlan`
  - `RuntimeActivationReport`
- include repo root, policy name, optional container name, repo override,
  session/lease policy summary, and stage list
- add pure unit tests for stage ordering and report shape
- do not wire runner callers yet

## Non-Goals

- no side-effect migration
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the crate builds, has focused tests, and no runner
behavior changes.

## Validation

- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Closeout

Added `crates/effigy-runtime-plan` with pure runtime activation request, plan,
stage, readiness, alias, lease, cleanup, and report types.

No runner behavior changed. The crate currently defines the activation stage
vocabulary and report identity shape only.

## Next Task

Start card
[`446-select-first-runtime-plan-runner-integration.md`](./446-select-first-runtime-plan-runner-integration.md).
