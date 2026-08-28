---
title: Papercuts wave 4 test routing worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / papercuts orchestrator
created: 2026-08-27
updated: 2026-08-27
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260827-235300-papercuts-wave4-test-routing.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

Waves 1–2 closed CLI `--` as a *global* catalog switch, Doctor built-ins,
graph timeout, worktree git mounts, extra-mount skip, and vault sharing.
Consumer filings still show task-arg `--` widening, suite-filter
confusion, `deps link bun` refusing registry symlinks, release gates
sorted by name, volume `in_use` lies, suite expansion dropping `run_in`,
and attention-marker CLI overrides ignored.

You are the Effigy implementation worker for this fourth lane. Operator
authorized. Do not invent a generation card.

## Why It Matters

Focused `effigy test:unit -- paths` still runs the full suite. `effigy
test stem` treats a package name as a Vitest filter. Release pays for the
expensive gate first. Worker worktrees cannot replace Bun's installed
symlink.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `834a4bdda87801fdcdc1745f53266ffb0ff9c10e`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved
  to that SHA before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator.
- **Worker branch:** `worker/papercuts-wave4-test-routing`
- **Worker worktree:** launcher first. Named fallback under
  `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`.
- **Ready work items, in order:**
  1. Effigy task arguments silently widen when preceded by `--` (Underlay
     `test:unit -- paths`)
  2. `effigy test <target>` passes the target as a suite filter
     (Songsprout)
  3. `deps link bun` refuses Bun registry package symlinks (Longhorn)
  4. Release gates execute in name order, so cheap-first is unbuyable
     (Longhorn)
  5. Volume list reports a running postgres volume as not in use
     (Contact Patch)
  6. Root test suite task-refs drop `run_in = "container"` (Contact Patch)
  7. Attention-marker CLI overrides are ignored (Underlay)
- **Out of scope:** Linux Docker Hub artifact rebuilds; catalog-member
  sibling hard-fail (intentional); recursive chown; Clippy image;
  isolation key; GitHub Release create on execute (protocol call);
  Finder metadata in Bun's own copy (remaining gap logged in Acowtancy).
- **Canonical refs:** `AGENTS.md`; `PAPERCUTS.md`; test orchestrator;
  `deps link bun`; `[release.gates]`; container volume list; scan
  attention-markers CLI.
- **Required validation:** `effigy test:unit -- <paths>` forwards or
  errors instead of widening; a package name is not applied as a Vitest
  filter across sibling suites; `deps link bun` can replace a registry
  symlink; gates run in declaration or explicit order without renaming
  (Longhorn asserts two gate lines verbatim); volume `in_use` true when
  the service is Up; suite expansion honors `run_in`; attention-marker
  flags change the rendered pattern lists (CLI contract test).
- **PR URL:** https://github.com/inflatable-cookie/effigy/pull/48
- **Merge authorisation:** absent; do not merge

## Boundaries

- Wave 1 already ended *global* `--` catalog switching. This lane is
  *task* argument forwarding after `--`.
- Do not rename Longhorn release gates to sort them.
- Do not merge.

## Important Context

- Consumer copies stay in those repos; log/close here.
- **Report to:** the operator.

## Suggested Next Move

Read this file, run the worktree preflight, then prove `test:unit -- paths`.

## Completion Protocol

### Before you start

1. Read this handoff. Run the four git identity commands.
2. Accept a clean dedicated non-`main` registered worktree.
3. Confirm `HEAD == origin/main` and ancestor
   `834a4bdda87801fdcdc1745f53266ffb0ff9c10e`.
4. Confirm this handoff exists in `HEAD`.

### When the assigned runway is complete

1. Close/add-and-close the seven items in this repo's `PAPERCUTS.md`.
2. Push a PR. Do not merge.

### Review and merge path

Awaiting orchestrator review. Merge is operator-authorised only.

- **Closeout refs:** `PAPERCUTS.md`; this handoff; the PR.

### Handoff closeout

If an item is already fixed on this SHA, close it with evidence.
