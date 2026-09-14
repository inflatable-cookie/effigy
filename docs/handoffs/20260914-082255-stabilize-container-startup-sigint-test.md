---
kind: northstar-handoff
title: "g10.005 — Stabilize the container startup SIGINT test"
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
owner: Tom
created: 2026-09-14
updated: 2026-09-14
base_required: pushed-main
roadmap: docs/roadmaps/g10/005-stabilize-container-startup-sigint-test.md
queue_dispatch: northstar-queue
queue_approval: "Operator-confirmed Effigy papercut planning intake on 2026-09-14 authorized promotion and coordinator execution preparation for bounded supported lanes."
queue:
  capability: general
  skipPRReview: false
  notifyOriginOnCloseout: false
---

## What This Thread Was Doing

Chatterbox reconciled the open Effigy papercuts against current runtime contracts. The production attached-interrupt outcome is already settled, but its startup SIGINT CLI test uses equal three-second child-delay and marker-wait budgets and fails intermittently under scheduling variance.

## Why It Matters

Unrelated worker validation can fail on a harness race even when production behavior is unchanged. The signal test needs to prove the intended startup window deterministically.

## Current State

- Canonical task: [`g10.005`](../roadmaps/g10/005-stabilize-container-startup-sigint-test.md).
- Planning base: `b3f9a9a38c3aa7b316629292a9868f1bd74fd020`.
- The integration checkout was clean and synchronized before promotion.
- The sibling [`g10.004`](../roadmaps/g10/004-place-log-index-entry-under-active-logs.md) is independent and may run concurrently.
- Portfolio skill sync remains triage-only and is not part of this handoff.

## Boundaries

Repair the test harness only. Do not change production signal handling, container lifecycle behavior, global test serialization, ignored-test policy, release surfaces, workflows, lifecycle projections, or unrelated tests. Stop if evidence points to a production defect or demands a wider source boundary.

## Important Context

`cli_container_attached_session_handles_sigint_during_startup` waits up to three seconds for the fake Colima invocation marker while the fake runtime itself is configured to sleep for three seconds. Current contracts require manager-owned clean interrupt closeout; they do not authorize altering that behavior to satisfy the test.

### UI Design Brief

Not applicable.

## Suggested Next Move

Trace marker creation and the controlled startup delay, then make the fixture announce entry into the exact interrupt window. Use bounded, separated budgets and retain the cleanup/no-log-follow assertions.

## Completion Protocol

Open one non-draft PR from the Queue workspace. Run at least ten repeated focused executions, the normal attached-session CLI test route, formatting, clippy, `effigy qa:docs`, and `git diff --check`. Independent review must confirm exact-head determinism, bounded waits, unchanged production semantics, and scope. After merge, let the required repository hook publish terminal state, refresh generated projections, and consume this exact handoff. Escalate only a decision-changing blocker.
