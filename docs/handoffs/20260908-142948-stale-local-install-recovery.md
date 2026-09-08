---
title: g09.009 stale local install recovery worker
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: northstar-queue
created: 2026-09-08
updated: 2026-09-08
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260908-142948-stale-local-install-recovery.md
base_required: pushed-main
queue_dispatch: northstar-queue
queue_approval: "Operator confirmed Chatterbox's recommendation on 2026-09-08: promote the stale local-install recovery as the final g09 lane, dispatch it through northstar-queue, close g09 after merge, then run Northstar Refresh."
queue:
  capability: general
  skipPRReview: false
tags: [coordination, handoff, worker, pr, bootstrap, diagnostics, self-hosting]
---

## What This Thread Was Doing

Chatterbox reconciled the merged Cargo version-transition lane, then promoted
the last evidenced g09 papercut: Effigy's stale repository-local binary can
reject new manifest grammar before it can run the task that replaces itself.

## Why It Matters

This is a self-hosting trust failure. The correct recovery already exists, but
the failing binary hides it behind an unrelated-looking TOML error. Shipping a
provenance-safe hint lets g09 close on its operator-clarity theme.

## Current State

- Canonical roadmap: `/Users/tom/Dev/projects/effigy/docs/roadmaps/g09/009-stale-local-install-recovery.md`
- Ready card: `/Users/tom/Dev/projects/effigy/docs/roadmaps/g09/batch-cards/1117-stale-local-install-recovery.md`
- Strict spec: `/Users/tom/Dev/projects/effigy/docs/specs/124-stale-local-install-recovery-strict-lane.md`
- Card `1116` is merged at `7d9c8be`; no Effigy queue task or sibling lane is
  active at dispatch.
- Owned paths, reserved closeout surfaces, evidence, and stop conditions are in
  the roadmap dispatch manifest. Required sibling worktree links: none.
- Capability: general automatic pool. Frontier-worker justification: none.

## Boundaries

Keep strict parsing strict. Do not add compatibility parsing, recognize a
particular unknown key, rebuild automatically, edit release/CI workflows, run a
release mutation, or change consumer repositories. Preserve unrelated work;
the primary checkout had user-owned `Cargo.lock` residue at promotion time.

## Important Context

The error occurs before `doctor` and `bootstrap:local` routing. The active
binary already exposes `+local.<sha>` identity through `effigy-core`; the hint
is allowed only when executable placement, repository identity, commit
resolution, and strict ancestry prove this checkout's local install is behind.
Unknown or divergent provenance retains the ordinary parse error. Use Effigy
selectors for validation and follow root `AGENTS.md`.

## Suggested Next Move

Start with the card's stale/current/false-positive fixture table. Trace
`effigy_manifest::ManifestError::Parse` into runner rendering and the existing
build-info helpers before choosing the smallest shared provenance seam.

## Completion Protocol

### Worker and PR loop

Implement only card `1117`, run its full oracle, self-review the exact diff,
commit, push, and open one non-draft PR against `main`. Report the PR URL and
exact head through the queue callback. Do not merge or edit reserved closeout
surfaces beyond the worker-owned evidence and docs named by the card.

### Review, closeout, and continuation

The queue coordinator owns independent exact-head review, merge, and canonical
closeout. After merge it marks roadmap `g09.009` and card `1117` complete,
archives spec `124`, updates every front door to `g09` closed with no ready
lane, deletes this handoff, runs docs QA, commits and pushes closeout, and sends
Chatterbox the final task/merge/closeout commits. Do not open `g10`; the operator
requested Northstar Refresh next, and Chatterbox owns that follow-up.
