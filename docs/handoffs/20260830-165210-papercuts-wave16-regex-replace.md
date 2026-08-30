---
title: Papercuts wave 16 regex::replace surface signature worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / papercuts orchestrator
created: 2026-08-30
updated: 2026-08-30
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260830-165210-papercuts-wave16-regex-replace.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

Northstar filed that `effigy rhai surface` documents
`regex::replace(value, pattern, replacement)`, but the live host and
in-repo callers use `(pattern, value, replacement)`. A caller who
trusts the catalog string gets a silent no-op that returns the pattern.

You are the Effigy implementation worker. Align the catalog string with
the live host. Do not flip the live argument order. Add-and-close the
papercut here; leave the Northstar copy for a later closeout.

## Why It Matters

Wrong-order calls look like they ran and leave the value unchanged.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `83e9c13f1fb4452c9d2be34cf4162d5e4cb01cc3`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved
  to that SHA before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator.
- **Worker branch:** `worker/papercuts-wave16-regex-replace`
- **Worker worktree:** launcher first.
  `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`.
- **Required sibling worktree links:** `none`
- **Ready work items, in order:**
  1. Effigy `regex::replace` surface signature is reversed —
     `crates/effigy-rhai/src/surface.rs` lists
     `regex::replace(value, pattern, replacement)` while
     `docs/guides/061-rhai-script-steps-guide.md` and
     `crates/effigy-rhai/src/tests/utility.rs` use
     `(pattern, value, replacement)`. Keep the live host order. Correct
     the catalog signature. Add a host self-check so catalog strings
     cannot drift from registered order. `regex::is_match` and
     `regex::captures` have the same catalog-vs-guide swap — treat
     those as the same membership bug if they still disagree with the
     live host. Do not change existing script call sites. Do not
     silently accept both orders.
- **Out of scope:** flipping the live API; Northstar consumer closeout;
  GitHub workflows; release mutations.
- **Canonical refs:** `PAPERCUTS.md` (add-and-close here);
  `crates/effigy-rhai/src/surface.rs`;
  `docs/guides/061-rhai-script-steps-guide.md`;
  Northstar `PAPERCUTS.md` open bullet (do not edit Northstar).
- **Required validation:** `effigy rhai surface` shows
  `regex::replace(pattern, value, replacement)` matching the live host.
  Focused rhai/surface tests pass, including a check that the catalog
  string matches the registered function.
- **PR URL:** pending
- **Merge authorisation:** absent; do not merge

## Boundaries

- Catalog and self-check only. Do not change live argument order. Do
  not merge.

## Important Context

- Filed from Northstar during the rust-quality setup wave. Fix the
  owning surface here.
- **Report to:** the operator.

## Suggested Next Move

Read this file from the top. Run the worktree-safety preflight. After
the committed `HEAD` handoff checks out, skip sibling links (`none`),
then align the regex catalog strings with the live host.

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
   `docs/handoffs/20260830-165210-papercuts-wave16-regex-replace.md`.
   Confirm `HEAD == origin/main`, ancestor
   `83e9c13f1fb4452c9d2be34cf4162d5e4cb01cc3`, and that relative path in
   `HEAD`. Load
   `git show HEAD:docs/handoffs/20260830-165210-papercuts-wave16-regex-replace.md`.
   If the absolute dispatch file differs, stop. The `HEAD` copy is
   canonical.
5. Required sibling list is `none`. Skip link setup.
6. Read `AGENTS.md` and `PAPERCUTS.md`.

### When the assigned runway is complete

1. Add-and-close the papercut in this repo's `PAPERCUTS.md`. Push a PR.
   Do not merge.

### Review and merge path

Awaiting orchestrator review. Merge is operator-authorised only.

- **Closeout refs:** `PAPERCUTS.md`; this handoff; the PR.

### Handoff closeout

Do not edit Northstar. Leave that copy open until a later closeout
cites this SHA.
