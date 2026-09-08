---
title: g09.009 stale local install recovery worker
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: closed
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
  notifyOriginOnCloseout: true
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
- Completed card: `/Users/tom/Dev/projects/effigy/docs/roadmaps/g09/batch-cards/1117-stale-local-install-recovery.md`
- Archived strict spec: `/Users/tom/Dev/projects/effigy/docs/specs/archive/124-stale-local-install-recovery-strict-lane.md`
- PR `95` merged to `main` at `24e842196465813f960cea15cadd25d5857731fd`.
- No Effigy queue task, ready card, active spec, or sibling lane remains.
- The handoff is retained as a closed task record; it is not a dispatch request.

## Boundaries

Keep strict parsing strict. Do not add compatibility parsing, recognize a
particular unknown key, rebuild automatically, edit release/CI workflows, run
a release mutation, or change consumer repositories. Preserve unrelated work;
the primary checkout had user-owned `Cargo.lock` residue at promotion time.

## Important Context

The error occurs before `doctor` and `bootstrap:local` routing. The active
binary already exposes `+local.<sha>` identity through `effigy-core`; the hint
is allowed only when executable placement, repository identity, commit
resolution, and strict ancestry prove this checkout's local install is behind.
Unknown or divergent provenance retains the ordinary parse error. Use Effigy
selectors for validation and follow root `AGENTS.md`.

## Suggested Next Move

No further g09 implementation work is authorized from this closed handoff.
Return to Chatterbox for the operator-requested Northstar Refresh; do not open
`g10` from this closeout.

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
lane, retains this handoff as a closed task record, runs docs QA, commits and
pushes closeout, and sends Chatterbox the final task/merge/closeout commits.
Do not open `g10`; the operator requested Northstar Refresh next, and
Chatterbox owns that follow-up.

## Closeout Record

- Accepted independent review covered exact head
  `5be2a1248b5862bf6f69d43dbd1dc25e07953224` and reported no blocking findings.
- Hosted checks and focused implementation validation passed; the dated
  evidence log records the stale/current/false-positive matrix and JSON/text
  behavior.
- The three deferred review observations are non-blocking: inaccurate test
  count wording, a fail-closed transient git re-probe, and `doctor` not
  rendering the stale note at the chosen parse-error boundary.
- The approved next pointer is the operator-requested Northstar Refresh
  through Chatterbox.
