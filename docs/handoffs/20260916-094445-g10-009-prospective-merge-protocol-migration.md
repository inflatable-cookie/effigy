---
title: g10.009 — Prospective-merge protocol migration
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
owner: Tom
created: 2026-09-16
updated: 2026-09-16
base_required: pushed-main
roadmap: docs/roadmaps/g10/009-prospective-merge-protocol-migration.md
queue_dispatch: northstar-queue
queue_approval: "Tom authorized the Northstar v4 protocol rollout across all consumer projects on 2026-09-16."
queue:
  capability: mechanical
  skipPRReview: false
  notifyOriginOnCloseout: true
---

## What This Thread Was Doing

Apply Northstar g03.020's accepted Queue manifest migration to Effigy.

## Why It Matters

The prospective merge target checks the tree Queue will merge and removes
reviewed-head false refusals caused by newer base changes.

## Current State

Effigy main is clean and synchronized. The installed migration dry-run accepts
its exact v3 manifest. Northstar g03.020 is terminal at `f34e1c0`. Required
sibling worktree links: none. Use the automatic economical pool;
frontier-worker justification: none.

## Boundaries

Change only `.paseo/queue.json` through the installed command. Do not change
Effigy runtime, catalogues, release state, CI, Queue state, product planning,
threads or workspaces.

## Important Context

Dry-run `effigy skill run northstar/lifecycle:migrate-premerge`, then apply with
`-- --write`. The only accepted edit is v3 to v4 plus `reviewed_head` to
`prospective_merge`. Refusals fail closed.

## Suggested Next Move

Verify the clean branch and committed handoff, run dry-run and write mode, then
inspect the complete diff before validation.

## Completion Protocol

Prove `applied`, then `unchanged` on replay, validate the v4 manifest, run the
focused docs/config checks and `git diff --check`. Commit and push one non-draft
PR, then report `ready_for_review` through Queue. Independent review uses a
different provider/model. Do not merge or publish Effigy.
