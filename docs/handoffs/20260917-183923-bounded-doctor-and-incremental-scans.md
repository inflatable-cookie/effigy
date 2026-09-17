---
kind: northstar-handoff
title: "g10.010 — Bounded doctor and incremental scans"
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
owner: Tom
created: 2026-09-17
updated: 2026-09-17
base_required: pushed-main
roadmap: docs/roadmaps/g10/010-bounded-doctor-and-incremental-scans.md
queue_dispatch: northstar-queue
queue_approval: "Operator approved the bounded default doctor, explicit deep mode, catalog-scoped shared inventory, exact incremental cache, and deadline plan on 2026-09-17 with 'Good plan, go for it'."
queue:
  capability: complex
  skipPRReview: false
  notifyOriginOnCloseout: false
---

## What This Thread Was Doing

Chatterbox investigated why `effigy doctor` times out in the large Acowtancy
monorepo. Current doctor serially combines structural checks, five default
whole-workspace content scans, and an eleven-step repository `health` aggregate.
Cold non-interactive callers can receive no useful evidence before timeout.

## Why It Matters

Doctor is the routing and repository-health entry point for agents. It must be
cheap enough to call during ambiguity and explicit when deeper repository work
is requested. Large catalogued monorepos must not rescan unrelated products or
repeat the same tree walk for each evaluator.

## Current State

- Canonical task: [`g10.010`](../roadmaps/g10/010-bounded-doctor-and-incremental-scans.md).
- Governing architecture: [`029`](../architecture/029-bounded-doctor-and-scan-cache.md).
- Governing contract: [`047`](../contracts/047-bounded-doctor-and-scan-cache-contract.md).
- `g10.008` and `g10.009` are complete. This is the sole ready frontier.
- Planning base is the pushed commit containing this handoff.

## Boundaries

Implement the committed vertical slice. Default doctor is structural only;
deep mode owns scans and selected-scope health. Reuse effective catalog
membership. Preserve standalone scan semantics, explanation mode, structural
`--fix`, task routing, and existing execution guarantees. Do not add discovery,
remote cache, pruning, daemon behavior, policy changes, workflow edits, or
release mutation.

## Important Context

Fast and deep overall defaults are 10 and 120 seconds. One selected scope gets
one content walk. Cache trust requires exact Git blob or content identity, not
mtime/size. Root scope prunes members; all-catalog fan-out is explicit. A
timeout is a non-zero incomplete report and must terminate/reap health work.
Prior valid cache state survives interrupted publication.

### UI Design Brief

Not applicable.

## Suggested Next Move

Land the orchestration split and flag conflicts first. Then establish scope and
single-walk instrumentation before cache reuse. Add deadline/process cleanup
before final report wiring so timeout tests exercise the real path.

## Completion Protocol

Open one non-draft PR from the Queue workspace. Run the task's focused
default/deep, traversal, cache identity/corruption, catalog isolation,
timeout/process-tree, report, JSON, help/completion, scan parity, and regression
proofs plus affected-crate tests, workspace tests, fmt, clippy, docs QA, JSON
contracts, and diff check. Independent exact-head review must attack silent
default work, repeated walks, stale cache hits, sibling leakage, partial cache
publication, orphan health children, and accidental compatibility drift. After
merge, let the lifecycle hook publish terminal state and consume this handoff.
