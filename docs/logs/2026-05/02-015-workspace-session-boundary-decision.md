# 02 015 Workspace Session Boundary Decision

Date: 2026-05-02
Roadmap: `g03.015`
Spec: `docs/specs/029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md`
Batch: `341`

## Decision

Keep `g03.015` open for one more bounded split slice.

## Why

`340` gave public workspace entry and bootstrap start handoff one explicit
session owner.

But these still sit in the same `workspace.rs` hotspot:

- Linux effigy artifact install and binary provisioning
- workspace permission preparation
- the prep glue between those steps and the public handoff path

That is still central enough to the lane goal that handing off now would be
early.
