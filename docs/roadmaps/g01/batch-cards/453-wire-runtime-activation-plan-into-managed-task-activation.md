# 453 - Wire Runtime Activation Plan Into Managed Task Activation

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make managed task container activation build a `RuntimeActivationPlan` before
calling existing runtime/container side effects.

## Scope

- build runtime activation plans in managed task activation helpers
- map managed lifecycle session policy into the activation plan
- preserve existing managed setup, readiness, handoff, cleanup, and output
  behavior
- add or adjust focused tests for plan identity and lease-policy mapping
- keep the side-effect migration for a later card

## Non-Goals

- no runtime prep stage extraction
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when managed task activation constructs runtime
activation plans and existing managed execution behavior remains unchanged.

## Closeout

- Managed task container policy handoffs now build `RuntimeActivationPlan`
  values before validation or rendered lifecycle command materialization.
- Named managed activation maps the current runtime session lease policy into
  the plan.
- Inline managed activation maps to skip-refresh, matching the existing
  host-lease behavior for inline workspace containers.
- Existing managed setup, readiness, shell handoff, cleanup, and output
  behavior remains unchanged.

## Validation

- `cargo test -p effigy --lib execute`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Select the first runtime-prep stage migration slice.
