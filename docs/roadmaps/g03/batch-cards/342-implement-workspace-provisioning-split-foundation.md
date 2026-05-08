# 342 Implement Workspace Provisioning Split Foundation

Status: archived
Updated: 2026-05-02
Roadmap: `g03.015`
Spec: `docs/specs/029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md`

## Objective

Split workspace provisioning and prep out of the remaining `workspace.rs`
hotspot.

## In Scope

- extract the next bounded workspace/runtime surface from `workspace.rs`
- center that slice on:
  - Linux effigy artifact install and binary provisioning
  - workspace permission preparation
  - the shared prep glue between those steps and public handoff
- reduce caller-local sequencing around those provisioning/prep steps

## Out Of Scope

- full workspace.rs decomposition
- runtime error taxonomy
- new runtime/container features

## Acceptance Criteria

- one clearer provisioning/prep owner exists
- workspace artifact install and permission prep no longer live as loose
  caller-local sequencing inside the main workspace hotspot
- public workspace handoff still behaves the same after the split

## Validation

- targeted workspace/session tests
- targeted bootstrap handoff tests
- targeted workspace artifact/provisioning tests
- `./target/debug/effigy docs check-paths docs/specs/029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md docs/roadmaps/g03/batch-cards/342-implement-workspace-provisioning-split-foundation.md docs/roadmaps/g03/batch-cards/343-decide-post-workspace-provisioning-split-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/015-workspace-runtime-orchestrator-split-and-handoff-simplification.md`

## Next Task

Closed. Execute `343`.
