---
title: Papercuts wave 20 git-fetch SSH timeout worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: awaiting-review
owner: Tom / papercuts orchestrator
created: 2026-08-30
updated: 2026-08-30
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260830-211120-papercuts-wave20-git-fetch.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

Worker preflight `git fetch origin` sat silent for minutes on a blocked
SSH prompt. A retry with
`GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes"` returned
immediately.

Northstar wave 20 owns the shared worker-handoff template wrap in
parallel. This Effigy lane documented the same fail-fast fetch on the
repo instruction surface and closed the papercut. No Git wrapper binary.

## Why It Matters

Startup probes look wedged when GitHub SSH waits on a prompt.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `f3057b9bb554f1a54b4c2d4cab2df27d5f6da202`
- **Worker mode:** implementation complete; do not dispatch another
  worker for this lane.
- **Worker branch:** `t3code/fix-git-fetch-papercut`
- **Worker worktree:** `/Users/tom/.t3/worktrees/effigy/t3code-39ad67e3`
- **Required sibling worktree links:** `none`
- **Completed work:**
  1. `AGENTS.md` documents
     `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`
     for worker-mode fail-fast fetch.
  2. `PAPERCUTS.md` closed
     `` `git fetch origin` can hang indefinitely waiting on SSH ``
     (2026-08-30).
- **Out of scope (still open elsewhere):** portfolio-level vendored-skill
  status/sync; GitHub workflows; release mutations; editing Northstar.
- **Canonical refs:** `PAPERCUTS.md`; `AGENTS.md`; this handoff; the PR.
- **Validation:**
  - `AGENTS.md` names the exact BatchMode + ConnectTimeout wrap.
  - Papercut is Closed.
  - Reviewer validation at `01251e71559814ce10236547ad43a829decbaa9c`:
    `git diff --check` passed; `effigy qa:docs` passed; `effigy doctor`
    completed with no errors (one existing god-files warning); seven
    GitHub checks green.
- **PR URL:** https://github.com/inflatable-cookie/effigy/pull/59
- **Merge authorisation:** absent; do not merge

## Boundaries

- Docs-only fail-fast fetch note. Do not wrap Git in a new tool.
- Do not merge without operator authorisation.

## Important Context

- Northstar wave 20 template work is parallel and not a blocker for this
  docs closeout.
- **Report to:** the operator.

## Suggested Next Move

Orchestrator review of https://github.com/inflatable-cookie/effigy/pull/59.
Do not launch another implementation worker. Merge is operator-authorised
only.

## Completion Protocol

### Review and merge path

Awaiting orchestrator review. Merge is operator-authorised only.

- **Closeout refs:** `PAPERCUTS.md`; `AGENTS.md`; this handoff; the PR.

### Handoff closeout

Leave portfolio skill sync open. No further worker startup for this lane.
