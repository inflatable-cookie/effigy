---
title: Papercuts wave 2 worktree/graph worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / papercuts orchestrator
created: 2026-08-27
updated: 2026-08-27
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260827-181200-papercuts-wave2-worktree-graph.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

Wave 1 closed CLI `--` passthrough, Doctor built-in `docs`, inline `{ rhai
= ... }`, and the contracts tempfile flake. A fresh collection still has
those same pains filed in consumer repos, plus worktree/graph/Bun
blockers that wave 1 left parked.

You are the Effigy implementation worker for this second lane. Do not
repeat wave 1. Operator-authorized papercuts runway; do not invent a
generation card.

## Why It Matters

Workers still cannot graph-explore honestly, run git inside a container
worktree, or install `file:` deps when Finder has dropped `.DS_Store`.
That burns a turn on every consumer lane.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `9975ff4e0fe736838059d57731f591a0c4c3b08f`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved
  to that SHA before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator.
- **Planning artifacts included at the base:** `PAPERCUTS.md`; this handoff.
- **Worker branch:** `worker/papercuts-wave2-worktree-graph`
- **Worker worktree:** prefer the launcher worktree. Named fallback:
  `/Users/tom/Dev/worktrees/effigy-papercuts-wave2-worktree-graph`
- **Worktree creation command:** only if preflight permits:
  `git worktree add /Users/tom/Dev/worktrees/effigy-papercuts-wave2-worktree-graph -b worker/papercuts-wave2-worktree-graph origin/main`
- **Worker worktree policy:** use a clean dedicated non-`main` launcher
  worktree regardless of generated path. `.agents.local.env` has
  `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`.
- **Active spec lane:** none. Do not create a spec or batch card.
- **Roadmap milestone:** none.
- **Ready work items, in order:**
  1. Make Effigy graph failures bounded and observable (filed in Acowtancy)
  2. Effigy graph indexes installed frontend output (filed in Figmatic)
  3. Container gitdir is host worktree path (filed in Composer)
  4. Container ops / catalog members hard-fail when a sibling mount is
     absent (filed in Contact Patch / Acowtancy)
  5. Bun container install dies on macOS Finder metadata in `file:` deps
     (filed in Acowtancy)
  6. Launcher worktrees miss the Effigy local vault (filed in Acowtancy)
- **Allowed runway:** those six items only, one PR.
- **Remaining card budget:** six papercuts.
- **Dispatch topology:** serial inside Effigy; parallel with other wave-2
  repos.
- **Parallel safety check:** no shared files with other wave-2 workers.
- **Canonical refs:** `AGENTS.md`; `PAPERCUTS.md`;
  `docs/contracts/001-working-rules.md`;
  `docs/guides/076-code-graph-and-agent-workflows.md`;
  `docs/guides/063-container-system-guide.md`;
  `docs/guides/075-secrets-and-vault-guide.md`.
- **Model capability profile:** capable coding model, medium reasoning.
- **Tool/runtime restrictions:** do not edit `.github/workflows/`; do not
  run release mutations; do not edit consumer repos in this PR.
- **Required validation:** a graph explore that would have stalled now
  returns a JSON error or completes; graph index skips `node_modules` /
  `dist`; a worktree-shaped gitdir is usable from `effigy exec git` or is
  diagnosed clearly; a missing `../book`-style extra mount warns instead
  of aborting doctor/container status; Finder `.DS_Store` in a `file:`
  dep does not fail install; worktree vault miss is shared or documented.
  Then cheap `effigy health` / focused tests you actually needed.
- **PR base/head:** current pushed `main` / selected worker branch
- **PR URL:** pending
- **Review state:** awaiting orchestrator review after worker completion
- **Merge authorisation:** absent; do not merge

## Boundaries

- **In scope:** the six items, plus new `PAPERCUTS.md` entries in *this*
  repo if the friction is not already logged here. Closing the consumer
  copies is a later consumer closeout, not this PR.
- **Out of scope:** release-gate ordering; recursive chown; Clippy in the
  workspace image; `isolation` manifest key; volume `in_use`; root test
  suite dropping `run_in`; Linux artifact Docker Hub; `--` task-arg
  forwarding (wave 1 already landed CLI `--` end; if `test:unit -- paths`
  still widens, that is a follow-up, not this lane unless you prove it is
  the same bug).
- Graph: add a bounded timeout and emit index/daemon health in the JSON
  error envelope. Exclude `node_modules` and build output from the index.
- Gitdir: rewrite or mount so container git sees a container-visible path.
  Do not guess `/tmp`.
- Sibling mounts: warn and skip a missing non-catalog extra mount the way
  user-global library mounts already skip.
- Bun: skip `.DS_Store` / AppleDouble when copying `file:` deps.
- Vault: share the primary-checkout local vault with registered worktrees,
  or fail with an explicit host-side fallback. Do not invent a new secrets
  backend.
- Do not merge the PR.

## Important Context

- **Planning lineage:** papercuts wave 2 after wave 1 PR 45.
- **Consumer filings:** Acowtancy, Composer, Contact Patch, Figmatic.
  Fix Effigy; do not open those repos.
- **Report after:** graph; gitdir; sibling mounts; DS_Store; vault; then PR.
- **Report to:** the operator, who will relay progress to the orchestrator.

## Suggested Next Move

Read this file from the top. Run the worktree-safety preflight. Use the
launcher worktree if it is clean, dedicated, and not `main`.

Start with graph timeout/envelope; it is the cheapest proof.

## Completion Protocol

### Before you start

1. Read this handoff. Then run `git rev-parse --show-toplevel`,
   `git branch --show-current`, `git status --porcelain`, and
   `git worktree list --porcelain`.
2. If the current root is a registered worktree, status is empty, and the
   branch is not `main`, accept it.
3. Only if unusable, use the named worktree, then `.agents.local.env`.
   Never use `/tmp`. Never clean a dirty checkout.
4. Confirm `HEAD == origin/main`, confirm
   `git merge-base --is-ancestor 9975ff4e0fe736838059d57731f591a0c4c3b08f HEAD`,
   and confirm this handoff exists in `HEAD`.
5. Read `AGENTS.md` and `PAPERCUTS.md`.

### While you work

- Commit in meaningful chunks.
- Report through the operator after each item.
- Stop if a secrets or mount change needs an operator product call.

### When the assigned runway is complete

1. Run the validation named above.
2. Log the six items in this repo's `PAPERCUTS.md` as closed or newly
   captured-and-closed.
3. Push the worker branch and open a PR against current pushed `main`.
4. Report the PR URL. Do not merge.

### Review and merge path

Awaiting orchestrator review. Merge is operator-authorised only.

- **Closeout refs:** `PAPERCUTS.md`; this handoff; the PR.

### Handoff closeout

If an item is already fixed on this SHA, close it with evidence instead
of forcing a diff.
