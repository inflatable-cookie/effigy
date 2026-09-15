---
kind: northstar-handoff
title: "g10.006 — Catalog-scoped code graph"
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
owner: Tom
created: 2026-09-15
updated: 2026-09-15
base_required: pushed-main
roadmap: docs/roadmaps/g10/006-catalog-scoped-code-graph.md
queue_dispatch: northstar-queue
queue_approval: "Operator said 'Great, let's do it' on 2026-09-15 after confirming catalog-derived segmentation and renaming separate database storage to independent."
queue:
  capability: complex
  skipPRReview: false
  notifyOriginOnCloseout: false
---

## What This Thread Was Doing

Chatterbox shaped a bounded response to Acowtancy's cold whole-repository graph
timeouts. The operator rejected duplicate graph segment topology: existing
effective catalogs define graph scopes, while each member manifest declares only
whether it is segmented and whether its storage is independent.

## Why It Matters

Acowtancy is roughly 35 GB and contains logically separate products. A Bovine
Desktop query must not index Farmyard, Dairy, Cream, or any other sibling before
it can answer. Path filters after a global refresh do not solve that cost.

## Current State

- Canonical task: [`g10.006`](../roadmaps/g10/006-catalog-scoped-code-graph.md).
- Governing architecture: [`027`](../architecture/027-catalog-scoped-code-graph.md).
- Governing contract: [`045`](../contracts/045-catalog-scoped-code-graph-contract.md).
- Planning base is the pushed commit containing this handoff.
- `g10.001` through `g10.005` are complete; this is the sole ready frontier.

## Boundaries

Deliver the complete catalog-scoped graph vertical slice. Preserve explicit
catalog membership, unsegmented behavior, query time budgets, graph watch, JSON
compatibility, and docs-context authority. Do not add discovery, recursive
membership, arbitrary paths/stores, remote indexing, a daemon, release/workflow
changes, or unrelated scan behavior. V1 rejects segmented catalog roots outside
the workspace.

## Important Context

Current `GraphPaths::for_repo` owns one database/lock, `scan_repo_files` walks
the resolved root, and query-local path filters apply after loading the global
sets. Root resolution already promotes nested catalog workspaces, and task
routing already owns effective membership, aliases, and cwd-nearest selection.
Reuse those concepts; do not create segment names or a second membership map.

`[catalog.graph] segmented = true` means an independently lazy scope pruned from
its parent. `independent = true` means its own deterministic physical database
and lock. Shared segmented scopes remain independently lazy. Root queries search
only root/folded source; `--all-catalogs` is the sole fan-out. Documentation
context retains its `[docs_policy.graph]` corpus.

### UI Design Brief

Not applicable.

## Suggested Next Move

Start with manifest and scope-model tests, then make graph paths/walking/storage
accept the scope descriptor before wiring CLI selection. Keep the no-sibling-
touch fixture active through each layer so a shared database cannot accidentally
restore global refresh behavior.

## Completion Protocol

Open one non-draft PR from the Queue workspace. Run the task's focused manifest,
codegraph, runner/CLI, JSON, watch, timeout, and docs-context proofs plus fmt,
clippy, docs QA, fast JSON contracts, and diff check. Independent exact-head
review must test the negative no-sibling-work oracle and verify owned paths.
After merge, let the repository lifecycle hook publish terminal state, refresh
projections/front doors, and consume this handoff. Escalate any required support
for external catalog roots or a docs/code corpus conflict to Chatterbox.
