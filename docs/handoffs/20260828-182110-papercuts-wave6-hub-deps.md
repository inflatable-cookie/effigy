---
title: Papercuts wave 6 Docker Hub and Bun deps-status worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / papercuts orchestrator
created: 2026-08-28
updated: 2026-08-28
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260828-182110-papercuts-wave6-hub-deps.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

Wave 5 retargeted skill JSON and locked git-bundle HEAD reads. Consumer
filings still show linux workspace exec rebuilding `ubuntu:22.04` via
Docker Hub while the workspace is already Up, and `deps status` reporting
`manager: null` on a root Bun workspace.

You are the Effigy implementation worker for this sixth lane. Operator
authorized. Do not invent a generation card.

## Why It Matters

Contact Patch `cp-api/test:unit` and Composer package selectors abort on
a Hub DNS timeout even when `*-dev-workspace-1` is Up. Acowtancy cannot
see its root Bun workspace on `deps status`.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `552ef1b93283f69f24acf9c5757c7e2ffacb89fe`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved
  to that SHA before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator.
- **Worker branch:** `worker/papercuts-wave6-hub-deps`
- **Worker worktree:** launcher first.
  `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`.
- **Ready work items, in order:**
  1. Workspace container exec rebuilds a linux artifact via Docker Hub —
     reuse an existing linux workspace artifact when the stack is already
     running, or fail with a cached-image/offline hint instead of a Hub
     pull. Filed in Contact Patch and Composer
  2. Detect the root Bun workspace in Effigy dependency status —
     `effigy --json deps status` should recognize a root `package.json` +
     `bun.lock` / Bun workspaces without requiring an active local-link
     record. Filed in Acowtancy
- **Out of scope:** GitHub Release create on execute (protocol vs
  provider-publication; do not choose); worktree bind-mounts of main;
  catalog-member sibling hard-fail (intentional); recursive chown;
  Clippy in the workspace image; Finder metadata in Bun's own `file:`
  copy.
- **Canonical refs:** `AGENTS.md`; `PAPERCUTS.md`; container exec /
  linux artifact build; `crates/effigy-deps` Bun discovery.
- **Required validation:** with a running workspace container, a
  `run_in = "container"` selector does not require a Hub pull of
  `ubuntu:22.04`. `deps status` on a root Bun workspace reports bun, not
  `manager: null`. Add/close both items in this repo's `PAPERCUTS.md`.
  Do not run `release prepare/execute`. Never modify
  `.github/workflows/`.
- **PR URL:** pending
- **Merge authorisation:** absent; do not merge

## Boundaries

- Reuse or fail-closed with a hint. Do not skip catalog members.
- Do not merge.

## Important Context

- Filed in Contact Patch, Composer, and Acowtancy; the files to change
  live here. Consumer copies stay in those repos.
- **Report to:** the operator.

## Suggested Next Move

Read this file, run the worktree preflight, then stop the Hub rebuild
when the workspace is already Up. Then teach `deps status` the root Bun
workspace.

## Completion Protocol

### Before you start

1. Read this handoff. Run `git rev-parse --show-toplevel`,
   `git branch --show-current`, `git status --porcelain`, and
   `git worktree list --porcelain`.
2. Accept a clean dedicated non-`main` registered worktree. Record the
   actual path/branch.
3. Confirm `HEAD == origin/main` and ancestor
   `552ef1b93283f69f24acf9c5757c7e2ffacb89fe`.
4. Confirm this handoff exists in `HEAD`.

### When the assigned runway is complete

1. Close/add-and-close the two items in this repo's `PAPERCUTS.md`.
2. Push a PR. Do not merge.

### Review and merge path

Awaiting orchestrator review. Merge is operator-authorised only.

- **Closeout refs:** `PAPERCUTS.md`; this handoff; the PR.

### Handoff closeout

If an item is already fixed on this SHA, close it with evidence.
