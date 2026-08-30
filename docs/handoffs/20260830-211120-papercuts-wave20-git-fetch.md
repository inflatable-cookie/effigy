---
title: Papercuts wave 20 git-fetch SSH timeout worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
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

Northstar wave 20 owns the worker-handoff template wrap. You are the
Effigy implementation worker. Document the same fail-fast fetch on this
repo's instruction surface and close the copy. Do not invent a Git
wrapper binary.

## Why It Matters

Startup probes look wedged when GitHub SSH waits on a prompt.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `f3057b9bb554f1a54b4c2d4cab2df27d5f6da202`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved
  to that SHA before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator.
- **Worker branch:** `worker/papercuts-wave20-git-fetch`
- **Worker worktree:** launcher first.
  `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`.
- **Required sibling worktree links:** `none`
- **Ready work items, in order:**
  1. `git fetch origin` can hang indefinitely waiting on SSH — put the
     BatchMode + ConnectTimeout wrap on `AGENTS.md` (or the worker
     preflight note it already points at). Close the papercut. Do not
     change `.github/workflows/`. Do not add a new Effigy command. Do
     not implement portfolio skill sync.
- **Out of scope:** portfolio-level vendored-skill status/sync; GitHub
  workflows; release mutations; editing Northstar.
- **Canonical refs:** `PAPERCUTS.md`; `AGENTS.md`.
- **Required validation:** `AGENTS.md` names
  `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes"` for
  worker `git fetch`. The papercut is closed.
- **PR URL:** pending
- **Merge authorisation:** absent; do not merge

## Boundaries

- Document fail-fast fetch. Do not wrap Git in a new tool. Do not merge.

## Important Context

- Northstar wave 20 is updating the shared worker template in parallel.
  That is not a blocker for this docs closeout.
- **Report to:** the operator.

## Suggested Next Move

Read this file from the top. Run the worktree-safety preflight. After
the committed `HEAD` handoff checks out, skip sibling links (`none`),
then document the wrap and close the papercut.

## Completion Protocol

### Before you start

1. Read this handoff path. Its `worker_mode: implementation` and
   `dispatch_authority: orchestrator` metadata activate worker mode. Then
   run `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. If the current root is a registered worktree, status is empty, and the
   branch is not `main`, accept it. Record the actual path/branch.
3. If the launcher supplied a dirty or `main` worktree, stop and report
   it. Fallback container is
   `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`. Never use
   `/tmp`.
4. From the selected worktree, record the repository-relative path
   `docs/handoffs/20260830-211120-papercuts-wave20-git-fetch.md`.
   Confirm `HEAD == origin/main`, ancestor
   `f3057b9bb554f1a54b4c2d4cab2df27d5f6da202`, and that relative path in
   `HEAD`. Load
   `git show HEAD:docs/handoffs/20260830-211120-papercuts-wave20-git-fetch.md`.
   If the absolute dispatch file differs, stop. The `HEAD` copy is
   canonical.
5. Required sibling list is `none`. Skip link setup.
6. Read `AGENTS.md` and `PAPERCUTS.md`.

### When the assigned runway is complete

1. Close the papercut in `PAPERCUTS.md`. Push a PR. Do not merge.

### Review and merge path

Awaiting orchestrator review. Merge is operator-authorised only.

- **Closeout refs:** `PAPERCUTS.md`; this handoff; the PR.

### Handoff closeout

Leave portfolio skill sync open.
