---
kind: northstar-handoff
title: "g10.004 — Place log-index entries under Active logs"
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
owner: Tom
created: 2026-09-14
updated: 2026-09-14
base_required: pushed-main
roadmap: docs/roadmaps/g10/004-place-log-index-entry-under-active-logs.md
queue_dispatch: northstar-queue
queue_approval: "Operator-confirmed Effigy papercut planning intake on 2026-09-14 authorized promotion and coordinator execution preparation for bounded supported lanes."
queue:
  capability: general
  skipPRReview: false
  notifyOriginOnCloseout: false
---

## What This Thread Was Doing

Chatterbox reconciled the open Effigy papercuts against current code and documentation authority. The docs-policy helper still recognizes only an obsolete archive marker and otherwise appends at EOF, which can place a new log entry below `## Next Task`.

## Why It Matters

The command reports success and satisfies uniqueness checks while corrupting the logs front-door structure. Workers then need an undeclared manual repair during closeout.

## Current State

- Canonical task: [`g10.004`](../roadmaps/g10/004-place-log-index-entry-under-active-logs.md).
- Planning base: `b3f9a9a38c3aa7b316629292a9868f1bd74fd020`.
- The integration checkout was clean and synchronized before promotion.
- The sibling [`g10.005`](../roadmaps/g10/005-stabilize-container-startup-sigint-test.md) is independent and may run concurrently.
- Portfolio skill sync remains triage-only and is not part of this handoff.

## Boundaries

Own only the paths in the task dispatch manifest. Preserve command grammar, report schemas, path normalization, index validation, unrelated docs, lifecycle projections, release surfaces, and workflows. Do not merge consumer policy or infer another destination when `## Active logs` is missing or ambiguous.

## Important Context

`crates/effigy-docs-policy/src/lib.rs::insert_log_index_entry` currently inserts before `## Archived Validation Logs` or appends to EOF. The current canonical log index uses `## Archived logs`, `## Active logs`, and a later `## Next Task`. The repair must target the Active logs section itself and fail closed on malformed structure.

### UI Design Brief

Not applicable.

## Suggested Next Move

Start with focused docs-policy fixtures that express the current README structure, then change the owner helper and strengthen the subprocess proof. Keep the diff inside the declared lane.

## Completion Protocol

Open one non-draft PR from the Queue workspace. Run the task's focused tests, formatting, clippy, `effigy qa:docs`, and `git diff --check`. Independent review must confirm exact-head section placement, invalid-structure behavior, idempotence, compatibility, and scope. After merge, let the required repository hook publish terminal state, refresh generated projections, and consume this exact handoff. Escalate only a decision-changing blocker.
