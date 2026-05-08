# 340 Implement Workspace Session Orchestrator Foundation

Status: archived
Updated: 2026-05-02
Roadmap: `g03.015`
Spec: `docs/specs/029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md`

## Objective

Carve out one clearer owner for public workspace/session orchestration.

## In Scope

- extract the first bounded workspace/session orchestration surface from
  `system_command/workspace.rs`
- center that slice on:
  - direct public workspace entry
  - bootstrap start handoff
  - shared ownership/cleanup resolution at the session boundary
- reduce caller-local lifecycle glue where those paths currently overlap

## Out Of Scope

- full workspace.rs decomposition
- artifact/binary staging split
- broad env shaping cleanup
- new runtime features

## Acceptance Criteria

- one explicit workspace/session orchestration owner exists
- direct workspace entry and bootstrap start handoff use that owner
- ownership and cleanup rules for that boundary are easier to trace than
  before

## Validation

- targeted workspace/session tests
- targeted bootstrap handoff tests
- `./target/debug/effigy docs check-paths docs/specs/029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md docs/roadmaps/g03/batch-cards/340-implement-workspace-session-orchestrator-foundation.md docs/roadmaps/g03/batch-cards/341-decide-post-workspace-session-orchestrator-foundation-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/015-workspace-runtime-orchestrator-split-and-handoff-simplification.md`

## Next Task

Closed. Execute `341`.
