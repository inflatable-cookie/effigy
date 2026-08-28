---
title: Papercuts wave 7 Clippy image and chown worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: awaiting-review
owner: Tom / papercuts orchestrator
created: 2026-08-28
updated: 2026-08-28
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260828-201710-papercuts-wave7-clippy-chown.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

Wave 6 reused the linux workspace artifact and taught `deps status` the
root Bun workspace. Acowtancy still files two generated-image gaps:
workspace images ship rustc without Clippy, and `effigy health` recursively
chowns every child `node_modules` tree.

You are the Effigy implementation worker for this seventh lane. Operator
authorized. Do not invent a generation card.

## Why It Matters

A clean container up cannot pass Farmyard validate without an extra
`rustup component add clippy`. Cheap health spends minutes on redundant
chown after root Bun consolidation.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `b58b93e17b382c0bbb02564f227faf8f3a15771f`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved
  to that SHA before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation complete; do not dispatch another worker.
- **Worker branch:** `t3code/fix-clippy-chown-papercuts`
- **Worker worktree:** `/Users/tom/.t3/worktrees/effigy/t3code-3abcf710`
- **Ready work items:** both implemented on the PR. Do not re-run them.
  1. Ship Clippy in the workspace container image — `workspace-rust-bun`
     Dockerfile installs Clippy; catalog contract test asserts the fragment
  2. Avoid recursive ownership prep across every workspace dependency
     tree — authoritative root `node_modules`/`vendor` stay recursive;
     child package trees are shallow; nested targets under a recursive
     parent are skipped; prep reports counts and per-path progress
- **Out of scope:** GitHub Release create on execute (protocol vs
  provider-publication; do not choose); worktree bind-mounts of main;
  catalog-member sibling hard-fail (intentional); Finder metadata in
  Bun's own `file:` copy; doctor scan-as-structural policy.
- **Canonical refs:** `AGENTS.md`; `PAPERCUTS.md`; generated workspace
  Dockerfiles / rustup fragments; container path preparation.
- **Required validation:** catalog fragment asserts `rustup component add
  clippy`. Permission-plan unit tests cover root-vs-child Bun trees and
  nested skip. Do not run `release prepare/execute`. Never modify
  `.github/workflows/`.
- **PR URL:** pending
- **Merge authorisation:** absent; do not merge

## Boundaries

- Generated image + path prep only. Do not skip catalog members.
- Do not merge.

## Important Context

- Filed in Acowtancy; the files to change live here. Consumer copies
  stay in that repo.
- **Report to:** the operator.

## Suggested Next Move

Orchestrator review of the PR. Merge is operator-authorised only.

## Completion Protocol

### Before you start

1. Read this handoff. Run `git rev-parse --show-toplevel`,
   `git branch --show-current`, `git status --porcelain`, and
   `git worktree list --porcelain`.
2. Accept a clean dedicated non-`main` registered worktree. Record the
   actual path/branch.
3. Confirm `HEAD == origin/main` and ancestor
   `b58b93e17b382c0bbb02564f227faf8f3a15771f`.
4. Confirm this handoff exists in `HEAD`.

### When the assigned runway is complete

1. Close/add-and-close the two items in this repo's `PAPERCUTS.md`.
2. Push a PR. Do not merge.

### Review and merge path

Awaiting orchestrator review. Merge is operator-authorised only.

- **Closeout refs:** `PAPERCUTS.md`; this handoff; the PR.

### Handoff closeout

If an item is already fixed on this SHA, close it with evidence.
