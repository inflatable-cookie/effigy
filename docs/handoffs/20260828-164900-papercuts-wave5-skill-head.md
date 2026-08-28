---
title: Papercuts wave 5 skill JSON and HEAD-serialize worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / papercuts orchestrator
created: 2026-08-28
updated: 2026-08-28
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260828-164900-papercuts-wave5-skill-head.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

Wave 4 closed test-arg `--`, catalog-vs-filter routing, `deps link bun`,
declaration-order release gates, volume `in_use`, suite `run_in`, and
attention-marker CLI overrides. Consumer copies still mention a stale
skill JSON path, and Acowtancy still flakes when two docs QA selectors
parse manifests in parallel.

You are the Effigy implementation worker for this fifth lane. Operator
authorized. Do not invent a generation card.

## Why It Matters

The installed skill still tells agents to query
`.result.payload.tasks[]`, so machine-readable task inventory fails
before they can filter ownership. Parallel `effigy docs/qa:*` can lose
`git rev-parse HEAD` with `ambiguous argument 'HEAD'`.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `02100eefdde17db64652b2b26317bb284c504d8e`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved
  to that SHA before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator.
- **Worker branch:** `worker/papercuts-wave5-skill-head`
- **Worker worktree:** launcher first.
  `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`.
- **Ready work items, in order:**
  1. Effigy task-inventory JSON example uses a stale payload path —
     live `effigy --json tasks` exposes `.result.catalog_tasks[]`.
     `skills/effigy/SKILL.md`, `references/json-envelope.md`, and
     `references/first-five-commands.md` still query
     `.result.payload.tasks[]`. Update the skill examples and any
     versioned envelope reference to the live schema
  2. Parallel `effigy docs/qa:*` can fail HEAD lookup — Acowtancy
     `docs/qa:docs` + `docs/qa:northstar` launched together died on
     `git rev-parse HEAD` / `ambiguous argument 'HEAD'`; serial retry
     passed. Prefer serializing git reads in manifest parse. If that is
     not a bounded fix, document that those selectors must not overlap
     and stop rather than inventing a task scheduler
- **Out of scope:** GitHub Release create on execute (protocol vs
  provider-publication; do not choose); worktree bind-mounts of main
  instead of the current worktree; catalog-member sibling hard-fail
  (intentional); Linux Docker Hub artifact rebuilds; recursive chown;
  Clippy in the workspace image; Finder metadata in Bun's own `file:`
  copy; `isolation` schema (Underlay already removed the table).
- **Canonical refs:** `AGENTS.md`; `PAPERCUTS.md`;
  `skills/effigy/SKILL.md`; `docs/guides/026-json-payload-examples.md`
  (already shows `catalog_tasks`); manifest parse / git identity.
- **Required validation:** `effigy --json tasks | jq -r
  '.result.catalog_tasks[].name'` matches the skill examples. A
  contract test or documented serialisation covers the HEAD flake if
  you change parse. Do not run `release prepare/execute`.
- **PR URL:** pending
- **Merge authorisation:** absent; do not merge

## Boundaries

- Docs/examples for item 1. Item 2 is a parse-serialisation fix or a
  documented overlap restriction, not a new scheduler.
- Never modify `.github/workflows/`. Never run release mutations.
- Do not merge.

## Important Context

- Filed in Underlay (skill JSON) and Acowtancy (HEAD flake); the files
  to change live here. Log/close in this repo's `PAPERCUTS.md`.
- Consumer closeouts in the same wave must not wait on this PR.
- **Report to:** the operator.

## Suggested Next Move

Read this file, run the worktree preflight, then retarget the skill JSON
examples. Reproduce the parallel HEAD flake if you can, then serialize
or document.

## Completion Protocol

### Before you start

1. Read this handoff. Run `git rev-parse --show-toplevel`,
   `git branch --show-current`, `git status --porcelain`, and
   `git worktree list --porcelain`.
2. Accept a clean dedicated non-`main` registered worktree. Record the
   actual path/branch.
3. Confirm `HEAD == origin/main` and ancestor
   `02100eefdde17db64652b2b26317bb284c504d8e`.
4. Confirm this handoff exists in `HEAD`.

### When the assigned runway is complete

1. Close/add-and-close the two items in this repo's `PAPERCUTS.md`.
2. Push a PR. Do not merge.

### Review and merge path

Awaiting orchestrator review. Merge is operator-authorised only.

- **Closeout refs:** `PAPERCUTS.md`; this handoff; the PR.

### Handoff closeout

If an item is already fixed on this SHA, close it with evidence.
