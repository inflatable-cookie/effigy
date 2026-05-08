# 343 Decide Post-Workspace-Provisioning Split Boundary

Status: archived
Updated: 2026-05-02
Roadmap: `g03.015`
Spec: `docs/specs/029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md`

## Objective

Decide whether `g03.015` needs another bounded split slice after workspace
provisioning and prep are split out.

## In Scope

- inspect the landed `342` surface against the roadmap and strict-lane target
- decide whether another bounded workspace/runtime split slice is still needed
- refresh the active strict-lane and front-door surfaces to match that
  decision

## Out Of Scope

- error taxonomy work
- crate extraction
- new runtime features

## Acceptance Criteria

- the next honest boundary after `342` is explicit
- no stale ready card is left behind
- the strict-lane state matches reality

## Validation

- `./target/debug/effigy docs check-paths docs/specs/029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md docs/roadmaps/g03/batch-cards/342-implement-workspace-provisioning-split-foundation.md docs/roadmaps/g03/batch-cards/343-decide-post-workspace-provisioning-split-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/015-workspace-runtime-orchestrator-split-and-handoff-simplification.md`

## Next Task

Closed. Promote `g03.016`.
