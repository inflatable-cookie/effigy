---
title: Named skill resolution and stdio passthrough handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
owner: northstar-queue
created: 2026-09-13
updated: 2026-09-13
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260913-100503-named-skill-resolution-stdio-passthrough.md
base_required: pushed-main
tags: [coordination, handoff, skill-run]
queue_dispatch: northstar-queue
queue_approval: "Operator requested named installed-skill resolution, supplied the raw stdio passthrough acceptance contract, and said continue on 2026-09-13."
queue:
  capability: general
  skipPRReview: false
---

## What This Thread Was Doing

Chatterbox converted the operator's installed-skill lookup request and
Northstar's raw stdio transport requirement into one ready g10 task. Planning
settled syntax, source precedence, transport ownership, failure behavior, and
the compatibility boundary.

## Why It Matters

An orchestrator must be able to invoke a task shipped by an installed agent
skill without hard-coding its filesystem path. Machine-to-machine callers also
need the selected task's bytes and status, not Effigy's human report or JSON
envelope. Together these changes make `effigy skill run` a deterministic,
generic execution link while preserving isolation.

## Current State

Here is the short version of where things stand:

- **Done:** contract, architecture, generation, task, and planning log promoted
- **Still open:** implement, prove, document, independently review, merge, and
  close `g10.001`
- **Active spec lane:** none
- **Current task:** `g10.001`
  (`/Users/tom/Dev/projects/effigy/docs/roadmaps/g10/001-named-skill-resolution-and-stdio-passthrough.md`)
- **Canonical refs:** contract 042, architecture 025, g10 README, and g10.001
- **Remaining continuation envelope:** this task only; return to Chatterbox
  after closeout
- **Lane budget / pause signal:** one task; stop on a new public compatibility
  or discovery decision
- **Key files:**
  - `/Users/tom/Dev/projects/effigy/docs/contracts/042-external-skill-task-runner-contract.md`
  - `/Users/tom/Dev/projects/effigy/docs/architecture/025-external-skill-task-execution.md`
  - `/Users/tom/Dev/projects/effigy/src/runner/skill_command.rs`
  - `/Users/tom/Dev/projects/effigy/src/cli/entrypoint.rs`

## Boundaries

Please keep the next pass within these boundaries:

- **In scope:** all work, paths, proofs, and closeout named by `g10.001`
- **Out of scope:** named `skill tasks`, network/registry acquisition,
  Northstar- or hook-specific schemas, consumer catalog merging, widened task
  shapes, workflows, releases, and speculative g10 follow-ons
- **Repo constraints:** Follow `/Users/tom/Dev/projects/effigy/AGENTS.md` and the
  canonical architecture/contracts named above. Use the project-local Effigy
  skill. Preserve unrelated work.

## Important Context

- **Planning lineage:** operator-selected Horizon C seam in vision 020; g10 is
  open with only g10.001 approved
- **How the plan fits the system:** source lookup extends the current isolated
  external-skill runner; passthrough changes only transport after preflight
- **Decisions and preferences:** `--json` stays Effigy's versioned envelope;
  use `--stdio passthrough` for child-owned raw streams. Explicit `--path` wins.
  Discovery uses the invocation project, never `--repo`.
- **Open tensions:** exact exit propagation crosses the current string-returning
  CLI rendering seam. Keep the solution bounded and return if it requires a
  broad error/output rewrite.

### UI Design Brief

Not applicable.

## Suggested Next Move

Read `g10.001`, contract 042, architecture 025, and the project-local Effigy
skill. Inspect the CLI entry/output and execution process seams, then implement
the smallest typed mode that preserves normal output and all preflight gates.

## Completion Protocol

Before finishing:

1. Complete every acceptance row and record exact validation.
2. Open or update the worker PR and resolve independent review findings.
3. Merge only after review accepts the exact head.
4. Update g10.001 outcome/evidence/status and every reserved front door.
5. Write one closeout log and delete this transient handoff.
6. Leave Chatterbox as the next task; do not create g10.002.

Disposition trigger: delete after successful canonical closeout. Git and
Northstar Queue retain the execution record.
