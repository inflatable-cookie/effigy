---
kind: northstar-handoff
title: "g10.008 — Published and draft task surfaces"
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
owner: Tom
created: 2026-09-15
updated: 2026-09-15
base_required: pushed-main
roadmap: docs/roadmaps/g10/008-published-and-draft-task-surfaces.md
queue_dispatch: northstar-queue
queue_approval: "Operator confirmed the proposed published `[tasks]` and lifecycle-labelled `[drafts]` split on 2026-09-15 with 'That works for me'."
queue:
  capability: complex
  skipPRReview: false
  notifyOriginOnCloseout: false
---

## What This Thread Was Doing

Chatterbox addressed recurring task-catalog bloat: temporary proof and test-
environment definitions accumulate beside stable contributor commands until
normal discovery becomes unwieldy. The operator selected a published/draft
split rather than per-task visibility flags in one undifferentiated library.

## Why It Matters

Humans and agents need a small trustworthy default command surface. Temporary
work still needs Effigy's real execution, isolation, container, managed-session,
and status guarantees, but it must be visibly provisional and removable without
creating dependencies from maintained public commands.

## Current State

- Canonical task: [`g10.008`](../roadmaps/g10/008-published-and-draft-task-surfaces.md).
- Governing architecture: [`028`](../architecture/028-published-and-draft-task-surfaces.md).
- Governing contract: [`046`](../contracts/046-published-and-draft-task-surface-contract.md).
- `g10.006` and `g10.007` are complete; this is the sole ready frontier.
- Planning base is the pushed commit containing this handoff.

## Boundaries

Deliver the committed published/draft vertical slice. Preserve `[tasks]`, flat
task routing, existing text/JSON/status behavior, catalog precedence, and the
canonical task execution pipeline. Do not add local ignored drafts, implicit
directory discovery, generation, pruning, enforced expiry, access control,
automatic migration, workflow/release edits, or a second runner.

## Important Context

`[tasks]` remains published without a new flag. `[drafts]` entries require full
tables with strict `created`, optional `expires`, and non-empty `purpose`.
`effigy tasks` excludes them; `effigy drafts` inventories lifecycle and exact
manifest provenance; `effigy draft <selector>` runs one through ordinary
catalog routing and execution.

Published tasks cannot reference drafts. Drafts may reference published tasks;
draft-to-draft composition is explicit and never an unresolved-task fallback.
Expiry produces inventory/doctor evidence but does not disable, mutate, or
delete. Dated fragments are explicitly included through existing composition.

### UI Design Brief

Not applicable.

## Suggested Next Move

Start with the manifest/surface identity and whole-graph reference invariants,
then add listing and command routing before status and doctor integration. Keep
one unchanged-manifest compatibility fixture and one no-side-effect invalid-
reference fixture active through the implementation.

## Completion Protocol

Open one non-draft PR from the Queue workspace. Run the task's focused manifest,
routing, CLI, listing, completion, JSON, execution, status, doctor, docs, and
compatibility proofs plus workspace tests, fmt, clippy, docs QA, JSON contracts,
and diff check. Independent exact-head review must exercise the hidden-surface,
dependency-direction, expiry-no-mutation, and runtime-reuse counterexamples.
After merge, let the repository lifecycle hook publish terminal state, refresh
front doors/projections, and consume this handoff. Return any local-draft,
automatic-prune, expiry-enforcement, or access-control requirement to
Chatterbox.
