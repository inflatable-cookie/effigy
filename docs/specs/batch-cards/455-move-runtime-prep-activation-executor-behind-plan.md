# 455 - Move Runtime Prep Activation Executor Behind Plan

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make `container_runtime_prep` execute task activation from a
`RuntimeActivationPlan` instead of treating the plan as caller-side metadata.

## Scope

- update `activate_container_runtime_for_task` internals to build or consume a
  `RuntimeActivationPlan`
- keep `ActivationRequest` as a temporary runner compatibility shim if cheaper
- make the internal executor derive:
  - container name
  - repo override
  - lease refresh policy
  from the plan
- keep existing side-effect functions in place:
  - `ensure_container_runtime_prepared`
  - `ensure_task_container_gateway_ready`
  - host-container lease refresh
- add focused tests that prove the executor uses plan lease policy and report
  identity without live containers

## Non-Goals

- no compose/backend migration
- no gateway/alias stage extraction
- no mount-prep rewrite
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when task activation side effects are driven from a
`RuntimeActivationPlan` and existing activation callers keep the same behavior.

## Closeout

- `activate_container_runtime_for_task` now converts the temporary
  `ActivationRequest` shim into a `RuntimeActivationPlan`.
- The internal task activation executor derives repo root, container name, repo
  override, and lease policy from the plan.
- Existing side-effect functions still own runtime prep, gateway readiness, and
  host-container lease refresh.
- Focused tests prove side-effect ordering, skip-refresh behavior, and
  activation report identity without live containers.

## Validation

- `cargo test -p effigy --lib container_runtime_prep`
- `cargo test -p effigy --lib execute`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Extract runtime policy validation into the first named activation stage.
