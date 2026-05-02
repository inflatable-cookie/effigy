# 029 Workspace Runtime Orchestrator Split And Handoff Simplification Strict Lane

Status: complete
Updated: 2026-05-02
Roadmap: `g03.015`

## Context

`g03.014` closed the container assembly seam strongly enough to stop treating
generated compose policy mutation as the main runtime brittleness hotspot.

The next honest weakness is the workspace/runtime orchestrator itself:

- public workspace handoff
- artifact and env shaping
- runtime cleanup ownership
- startup and handoff sequencing

still overlap too heavily inside `system_command/workspace.rs` and adjacent
runner entrypoints.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/contracts/005-container-runtime-contract.md`
- `docs/roadmaps/g03/015-workspace-runtime-orchestrator-split-and-handoff-simplification.md`
- `docs/roadmaps/g03/README.md`

## Lane Focus

This lane owns:

- splitting the workspace/runtime orchestration hotspot into narrower owners
- making public workspace handoff and cleanup policy traceable through one
  clearer module boundary
- reducing caller-local lifecycle glue across workspace-facing entrypoints

This lane does not own:

- new runtime/container features
- catalog redesign
- error taxonomy cleanup beyond what the split requires
- broad crate extraction

## Current Posture

`strict-complete`

The correct implementation order is:

1. carve out one explicit owner for public workspace/session orchestration
2. move bootstrap start handoff and direct workspace entry onto that owner
3. separate session ownership from artifact/env staging concerns
4. hand off cleanly once the remaining `workspace.rs` surface is mostly command
   and handoff glue rather than another mixed ownership seam

## Integration Constraint

- keep the first batch centered on public workspace/session orchestration
- do not try to split every helper out of `workspace.rs` at once
- preserve current runtime behavior while changing internal ownership

## Continuation Chain

1. `340` — implement the workspace session orchestrator foundation
2. `341` — decide whether another bounded workspace/runtime split slice is needed
3. `342` — implement the workspace provisioning split foundation
4. `343` — decide whether the lane can hand off after provisioning split

## Exit Condition

This strict lane is complete when:

- public workspace/session handoff has one clearer owner
- the main workspace-facing runtime entrypoints delegate to narrower
  orchestration APIs
- cleanup and ownership rules are no longer spread across mixed-responsibility
  workspace code

## Next Task

Closed. Promote `g03.016`.
