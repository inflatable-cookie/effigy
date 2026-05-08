# 451 - Wire Runtime Activation Plan Into Deferral

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make deferred container execution build a `RuntimeActivationPlan` before calling
the existing activation side effects.

## Scope

- build a runtime activation request/plan in the deferral container execution
  path
- map `current_runtime_session_context()` lease policy into the plan
- preserve the existing `activate_container_runtime_for_task` side-effect path
- keep deferral command rendering, depth handling, local handoff, workspace
  permission prep, and output unchanged
- add or adjust focused tests where existing fixtures make that cheap

## Non-Goals

- no standard task activation migration
- no side-effect migration
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when deferred container execution constructs a runtime
activation plan and existing deferral behavior remains unchanged.

## Validation

- `cargo test -p effigy --lib deferral`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Closeout

Deferred container execution now builds a `RuntimeActivationPlan` before
calling the existing activation side effects. The plan carries deferral working
dir, policy name, container name, repo override, and mapped current lease
policy.

Deferral command rendering, depth handling, local handoff, workspace permission
prep, and output are unchanged.

## Next Task

Start card
[`452-wire-runtime-activation-plan-into-standard-task-activation.md`](./452-wire-runtime-activation-plan-into-standard-task-activation.md).
