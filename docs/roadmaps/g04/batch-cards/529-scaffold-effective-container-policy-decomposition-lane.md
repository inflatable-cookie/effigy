# 529 - Scaffold Effective Container Policy Decomposition Lane

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Open `g04.007` with a concrete decomposition inventory and select the first
safe module extraction.

## Scope

- audit `crates/effigy-containers/src/lib.rs`, `workspace.rs`,
  `policy_support.rs`, and `exec.rs`
- identify policy model/load/validation/project/inline-workspace seams
- identify workspace mount/host-integration/compose-rewrite seams
- update the strict lane with the first implementation slice
- do not move code yet unless the first extraction is mechanically obvious and
  very small

## Non-Goals

- no public export break
- no broad formatting churn
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the lane has an implementation inventory and one
ready extraction card.

## Closeout

The lane now has a concrete decomposition inventory. The first implementation
slice is policy model extraction because it is mostly type movement and creates
a stable home for later load, validation, project, inline-workspace, DNS, and
eject splits.

## Validation

- `wc -l crates/effigy-containers/src/lib.rs crates/effigy-containers/src/workspace.rs crates/effigy-containers/src/policy_support.rs crates/effigy-containers/src/exec.rs`
- `rg -n '^pub struct|^pub enum|^pub fn|^fn |^impl ' crates/effigy-containers/src/lib.rs crates/effigy-containers/src/workspace.rs crates/effigy-containers/src/policy_support.rs crates/effigy-containers/src/exec.rs`
- `git diff --check`

## Next Task

Start card
[`530-extract-effective-container-policy-model-module.md`](./530-extract-effective-container-policy-model-module.md).
