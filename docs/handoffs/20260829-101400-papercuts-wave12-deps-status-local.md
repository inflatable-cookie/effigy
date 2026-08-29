---
title: Papercuts wave 12 deps-status committed locals worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / papercuts orchestrator
created: 2026-08-29
updated: 2026-08-29
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260829-101400-papercuts-wave12-deps-status-local.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

Wave 10 / PR 52 fixed `deps link` root selection. From Figmatic,
`deps link cargo ../longhorn` still refuses with `pre-migration path
dependency` and Bun reports `committed-pin-active`, because Figmatic
already declares Longhorn through Cargo `path` deps and Bun `file:`
overrides. Those refusals are correct. `deps status` still cannot report
the local dependency that is already in force.

You are the Effigy implementation worker. Give `deps status` a read-only
way to report committed path and `file:` locals as observed links. Do
not make `deps link` rewrite a path dep or override a committed pin.

## Why It Matters

Figmatic already has the sibling Longhorn checkout wired. Agents still
cannot see that through `deps status`, so they try `deps link` and hit a
correct refusal.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `632f925432544521f5bedc04253ca53c8012d794`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved
  to that SHA before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator.
- **Worker branch:** `worker/papercuts-wave12-deps-status-local`
- **Worker worktree:** launcher first.
  `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`.
- **Required sibling worktree links:**
  - `figmatic` from `/Users/tom/Dev/projects/figmatic` as `../figmatic`
  - `longhorn` from `/Users/tom/Dev/projects/longhorn` as `../longhorn`
  Create when absent; reuse only a symlink that already resolves to that
  source; stop on any other existing path; never overwrite.
- **Ready work items, in order:**
  1. `deps link` cannot adopt committed path / `file:` local
     dependencies — keep the link refusals. Teach `deps status` (cargo
     and bun) to report those committed locals as observed links so a
     Figmatic checkout against `../longhorn` shows the path/`file:`
     Longhorn deps that are already in force
- **Out of scope:** making `[patch]` redirect a Cargo path dep; making
  an ephemeral Bun link outrank a committed override; `deps pin`; the
  `effigy-rhai` process-wide env race; GitHub workflows; release
  mutations.
- **Canonical refs:** `PAPERCUTS.md`; `docs/guides/077-local-dependency-linking.md`;
  `MatchDisposition::PreMigrationPath`;
  `bun_pin::matching_committed_overrides`; `deps status`.
- **Required validation:** from sibling Figmatic,
  `effigy --json deps status cargo` and `... bun` name the committed
  Longhorn path/`file:` locals. `deps link cargo ../longhorn` and
  `deps link bun ../longhorn` still refuse. Focused deps tests for both
  dispositions. Add/close this item in `PAPERCUTS.md`. Do not run
  `release prepare/execute`.
- **PR URL:** pending
- **Merge authorisation:** absent; do not merge

## Boundaries

- Read-only status/report. Do not change the link refusals.
- Do not merge.

## Important Context

- Filed after PR 52 against Figmatic. Consumer closeout of the old
  workspace-root copy is a separate Figmatic lane.
- **Report to:** the operator.

## Suggested Next Move

Read this file from the top. Run the worktree-safety preflight. After
the committed `HEAD` handoff checks out, create the sibling links, then
prove status vs link on the Figmatic/Longhorn pair.

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
   `docs/handoffs/20260829-101400-papercuts-wave12-deps-status-local.md`.
   Confirm `HEAD == origin/main`, ancestor
   `632f925432544521f5bedc04253ca53c8012d794`, and that relative path in
   `HEAD`. Load
   `git show HEAD:docs/handoffs/20260829-101400-papercuts-wave12-deps-status-local.md`.
   If the absolute dispatch file differs, stop. The `HEAD` copy is
   canonical.
5. Then create the sibling links from that tracked list. Canonicalize
   source and destination. Create when absent; reuse only a correct
   symlink; stop on conflict; never overwrite.
6. Read `AGENTS.md` and `PAPERCUTS.md`.

### When the assigned runway is complete

1. Close/add-and-close the item in this repo's `PAPERCUTS.md`.
2. Push a PR. Do not merge.

### Review and merge path

Awaiting orchestrator review. Merge is operator-authorised only.

- **Closeout refs:** `PAPERCUTS.md`; this handoff; the PR.

### Handoff closeout

If status already reports those locals on this SHA, close with evidence.
