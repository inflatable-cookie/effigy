# 452 - Wire Runtime Activation Plan Into Standard Task Activation

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make standard task routed activation build a `RuntimeActivationPlan` before
calling existing activation side effects.

## Scope

- build runtime activation plans in:
  - `activate_routed_container_runtime`
  - inline workspace activation helpers if cheap and contained
- map current runtime session lease policy into the routed activation plan
- preserve the existing `activate_container_runtime_for_task` side-effect path
- keep route decisions, auto-up behavior, workspace-seeded sessions, inline
  cleanup, and output unchanged
- add or adjust focused tests for plan identity and lease-policy mapping

## Non-Goals

- no activation side-effect migration
- no managed task activation migration
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when standard task activation constructs runtime
activation plans and existing standard execution behavior remains unchanged.

## Closeout

- `src/runner/execute/pipeline/standard.rs` now builds
  `RuntimeActivationPlan` values for routed container activation.
- Inline workspace standard activation also maps its skip-refresh session
  policy into the typed plan.
- Existing activation side effects still flow through
  `activate_container_runtime_for_task`.
- Focused tests assert repo override, container name, and lease-policy parity
  between the activation request and the new plan.

## Validation

- `cargo test -p effigy --lib execute`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Wire runtime activation planning into managed task activation.
