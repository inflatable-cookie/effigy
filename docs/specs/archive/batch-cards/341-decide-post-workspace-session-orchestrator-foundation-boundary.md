# 341 Decide Post-Workspace-Session-Orchestrator Foundation Boundary

Status: complete
Updated: 2026-05-02
Roadmap: `g03.015`
Spec: `docs/specs/029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md`

## Objective

Decide whether `g03.015` needs another bounded split slice after the workspace
session orchestrator foundation.

## Decision

Keep `g03.015` open for one more bounded split slice.

Why:

- `340` gave public workspace entry and bootstrap start handoff one explicit
  session owner
- but artifact install, binary provisioning, and permission/env preparation
  still sit in the same `workspace.rs` hotspot
- that is still central enough to the lane goal that handing off now would be
  early

## In Scope

- inspect the landed `340` surface against the roadmap and strict-lane target
- decide whether another bounded workspace/runtime split slice is still needed
- refresh the active strict-lane and front-door surfaces to match that
  decision

## Out Of Scope

- error taxonomy work
- crate extraction
- new runtime features

## Acceptance Criteria

- the next honest boundary after `340` is explicit
- no stale ready card is left behind
- the strict-lane state matches reality

## Validation

- `./target/debug/effigy docs check-paths docs/specs/029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md docs/specs/batch-cards/340-implement-workspace-session-orchestrator-foundation.md docs/specs/batch-cards/341-decide-post-workspace-session-orchestrator-foundation-boundary.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/015-workspace-runtime-orchestrator-split-and-handoff-simplification.md`

## Next Task

Execute `342`.
