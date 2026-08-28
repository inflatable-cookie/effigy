---
title: Papercuts wave 10 deps-link workspace roots worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / papercuts orchestrator
created: 2026-08-28
updated: 2026-08-28
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260828-230910-papercuts-wave10-deps-link-roots.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

Figmatic filed that `effigy deps link cargo ../longhorn --dry-run` walks
into Longhorn's non-member `examples/command-system-proof/rust/jetstream`
package, and the Bun form looks for `package.json` at the Figmatic repo
root instead of `studio/`. Effigy cannot register the sibling Longhorn
path/`file:` links or report them through `deps status`.

You are the Effigy implementation worker. Make `deps link` select
manifest/package roots and ignore Cargo packages that are not members of
the owning workspace. Do not edit Figmatic or Longhorn.

## Why It Matters

A Figmatic worktree cannot declare the Longhorn checkout it already
depends on, so `deps status` stays blind to those links.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `b7f78f301c30aac9ad89fd69a5c731323e2d7c4d`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved
  to that SHA before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator.
- **Worker branch:** `worker/papercuts-wave10-deps-link-roots`
- **Worker worktree:** launcher first.
  `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`.
- **Required sibling worktree links:**
  - `figmatic` from `/Users/tom/Dev/projects/figmatic` as `../figmatic`
  - `longhorn` from `/Users/tom/Dev/projects/longhorn` as `../longhorn`
  Create when absent; reuse only a symlink that already resolves to that
  source; stop on any other existing path; never overwrite.
- **Ready work items, in order:**
  1. `deps link cargo` walks non-workspace Cargo packages — from a
     Figmatic-shaped consumer, `effigy deps link cargo ../longhorn
     --dry-run` must not enter Longhorn's nested workspace at
     `examples/command-system-proof/rust/jetstream` (own `[workspace]`,
     not a Longhorn member). Link only members of the owning workspace.
  2. `deps link bun` requires `package.json` at the repo root — Figmatic
     Bun lives in `studio/`. Select the package/workspace root that
     actually has the manifest (`studio/` or a Bun workspace root), not
     the git root by default.
- **Out of scope:** editing Figmatic or Longhorn; `deps pin`; GitHub
  workflows; release mutations; Poodle pin drift; catalog-member skip.
- **Canonical refs:** `PAPERCUTS.md`; `docs/guides/077-local-dependency-linking.md`;
  `crates/effigy-deps`; Figmatic `studio/package.json`; Longhorn root
  `Cargo.toml` members vs nested example workspaces.
- **Required validation:** dry-run from the sibling Figmatic checkout
  against sibling Longhorn. Add/close the item in this repo's
  `PAPERCUTS.md`. A focused deps test covering nested non-member Cargo
  workspaces and a non-root Bun `package.json`. Do not run
  `release prepare/execute`.
- **PR URL:** pending
- **Merge authorisation:** absent; do not merge

## Boundaries

- Fix discovery in Effigy. Do not restructure Longhorn examples or move
  Figmatic's `package.json`.
- Do not merge.

## Important Context

- Filed in Figmatic PAPERCUTS; the files to change live here. The
  Figmatic copy stays open until a later consumer closeout against this
  pin.
- **Report to:** the operator.

## Suggested Next Move

Read this file from the top. Run the worktree-safety preflight. After
the committed `HEAD` handoff checks out, create the Figmatic and
Longhorn sibling links, then reproduce the two dry-runs.

## Completion Protocol

### Before you start

1. Read this handoff path. Its `worker_mode: implementation` and
   `dispatch_authority: orchestrator` metadata activate worker mode. Then
   run `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. If the current root is a registered worktree, status is empty, and the
   branch is not `main`, accept it. Record the actual path/branch.
3. If the launcher supplied a dirty or `main` worktree, stop and report
   it. Otherwise the fallback is
   `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`. Never use
   `/tmp`.
4. From the selected worktree, record the repository-relative path
   `docs/handoffs/20260828-230910-papercuts-wave10-deps-link-roots.md`.
   Confirm `HEAD == origin/main`, ancestor
   `b7f78f301c30aac9ad89fd69a5c731323e2d7c4d`, and that relative path in
   `HEAD`. Load `git show HEAD:docs/handoffs/20260828-230910-papercuts-wave10-deps-link-roots.md`.
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

If an item is already fixed on this SHA, close it with evidence.
