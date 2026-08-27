---
title: Papercuts wave 1 CLI/doctor/test flake worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / papercuts orchestrator
created: 2026-08-27
updated: 2026-08-27
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260827-160100-papercuts-wave1-cli-doctor.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

A cross-repo `effigy papercuts` sweep found 140 open entries. Many of the
painful ones were filed in consumer repos but belong here: global `--`
swallowing, Doctor treating built-in `docs` as a missing task, Doctor
rejecting the inline `{ rhai = ... }` task shape the runner already accepts,
and a contracts-test temp-dir collision.

The operator approved wave 1 and asked for one orchestrator handoff per
repo. You are the Effigy implementation worker for this lane only. Do not
use a copied transcript or a second prompt.

This is not a generation batch card. Papercuts stay observations until an
operator authorizes a bounded fix; that authorization is this handoff. Do
not invent a roadmap milestone.

## Why It Matters

Consumer workers cannot finish honest QA while Effigy itself mis-parses
`--`, paints Doctor red on valid built-ins, and flakes a contracts test.
Fixing those here unblocks Longhorn, Underlay, Underlay Reference,
Acowtancy, and Songsprout without asking those repos to workaround the CLI.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `aa0c6825e04f82830ed52e0660064dcd68dc4757`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved
  to that SHA before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight.
- **Planning artifacts included at the base:** `PAPERCUTS.md`; this handoff.
- **Worker branch:** `worker/papercuts-wave1-cli-doctor`
- **Worker worktree:** prefer the launcher-provided clean dedicated
  worktree. Named manual fallback:
  `/Users/tom/Dev/worktrees/effigy-papercuts-wave1-cli-doctor`
- **Worktree creation command:** only when the startup preflight permits the
  manual fallback:
  `git worktree add /Users/tom/Dev/worktrees/effigy-papercuts-wave1-cli-doctor -b worker/papercuts-wave1-cli-doctor origin/main`
- **Worker worktree policy:** first use a clean, dedicated, non-`main`
  registered worktree supplied by the launcher, even if its generated path
  or branch differs from these placeholders. Record the actual path/branch
  and never create a second worktree for that reason. If the current context
  is unusable, use the named worktree when it matches; only then read
  `.agents.local.env`, require `AGENTS_WORKTREE_CONTAINER_DIR`, and create a
  unique manual worktree/branch under that container from `origin/main`. Ask
  the operator first if the file or key is absent; never use `/tmp`,
  `TMPDIR`, or a guessed path.
- **Active spec lane:** none. Do not create a spec or batch card.
- **Roadmap milestone:** none. Do not open a papercuts generation.
- **Ready work items, in order:**
  1. Parallel contracts tests can share a timestamp temp directory
  2. Global `--` is consumed after `--`, so a task cannot take `--repo` as
     its own argument
  3. Doctor rejects built-in `docs` steps as unresolved task references
  4. Doctor schema rejects inline `{ rhai = ... }` task values that the
     runner accepts
- **Allowed runway:** those four items only, one PR.
- **Remaining card budget:** four papercuts.
- **Dispatch topology:** serial inside this repo; parallel with the other
  wave-1 repos.
- **Parallel safety check:** no shared files with the other wave-1 workers.
  Keep this lane inside Effigy.
- **Canonical refs:** `AGENTS.md`; `PAPERCUTS.md`;
  `docs/contracts/001-working-rules.md`;
  `docs/guides/016-task-routing-precedence.md`;
  `docs/guides/017-json-output-contracts.md`;
  `docs/guides/025-command-reference-matrix.md`.
- **Model capability profile:** capable coding model, medium reasoning.
- **Tool/runtime restrictions:** use Effigy selectors; do not edit
  `.github/workflows/`; do not run release mutations; do not start another
  worker or orchestrator.
- **Required validation:** focused contracts tests for the flake; Doctor
  against a fixture that uses `docs check` in a sequence and an inline
  `{ rhai = ... }` task; a CLI proof that `effigy <task> -- --repo <path>`
  no longer switches catalogs (or the chosen explicit error); then the
  repo's cheap health/docs checks you actually needed. Full `effigy qa` at
  closeout if the cheap gates were green.
- **PR base/head:** current pushed `main` / selected worker branch
- **PR URL:** pending
- **Review state:** awaiting orchestrator review after worker completion
- **Merge authorisation:** absent; do not merge

## Boundaries

- **In scope:** the four papercuts named above, plus closing those entries
  in `PAPERCUTS.md` with a one-line fix note.
- **Out of scope:** release-gate ordering; worktree gitdir inside
  containers; Bun `file:` copies / `.DS_Store`; graph daemon stalls; deps
  status for root Bun workspaces; sibling catalog mounts; recursive chown.
  Those are a later Effigy lane.
- Do not invent architecture, change the release protocol, or widen the
  papercut into a CLI redesign.
- For item 1: `crates/effigy-contracts/src/tests.rs` currently uses
  `tempfile::tempdir()`. Prove whether the nanosecond-identity collision
  still exists. If it is already gone, close the papercut with evidence
  rather than rewriting working tests.
- For item 2: `--` must end global flag parsing. Remaining args reach the
  task, including shell-string tasks. Do not break `effigy --repo <PATH>
  <task>` catalog switching when `--repo` is before the task name.
- For items 3–4: Doctor should accept built-in `docs` in sequence steps and
  the inline `{ rhai = "..." }` task table the runner already executes.
  Do not force Longhorn onto a different task syntax.
- Work only in the selected clean worker worktree. Never edit the
  orchestrator's planning checkout or clean/reset an unrelated dirty
  checkout.
- Do not merge the PR.

## Important Context

- **Planning lineage:** operator-authorized papercuts wave 1 from
  `/Users/tom/Dev/projects` on 2026-08-27. Consumer filings live in
  Longhorn (`--repo` after `--`, inline rhai Doctor schema), Underlay
  Reference (Doctor `docs` built-in), Acowtancy/Songsprout (`--`
  forwarding). Fix the CLI here; do not edit those consumer repos.
- **Why these items are ready:** each has a named surface, a plausible
  fix, and no product decision. Working-rules normally require a ready
  batch card; the operator explicitly authorized this papercuts runway
  instead of compiling a generation card.
- **Decisions and preferences:** keep the change small. Prefer ending
  global flag parsing at `--` over adding a second target-repo flag,
  unless that cannot be done without breaking catalog switching.
- **Open tensions:** a full Doctor green is not required if pre-existing
  scan findings remain. Do not start a god-file splitdown.
- **Report after:** first the contracts flake proof; then `--` parsing;
  then the two Doctor schema/built-in fixes; then PR.
- **Report to:** the operator, who will relay progress to the orchestrator.

## Suggested Next Move

Read this file from the top. Before broad repository reads, run the
worktree-safety preflight in `## Completion Protocol`. If the current
context is a clean, dedicated, non-`main` registered worktree, use it
immediately, record its actual path/branch, and do not create another
worktree.

Then read `AGENTS.md`, `PAPERCUTS.md`, and the four surfaces. Start by
proving whether the contracts flake still exists, then fix `--` parsing,
then Doctor.

## Completion Protocol

### Before you start

1. Read this handoff path. Its `worker_mode: implementation` and
   `dispatch_authority: orchestrator` metadata activate worker mode. Then
   run one quick read-only safety probe before broad repository reads:
   `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. If the current root is a registered worktree, its status is empty, and
   its branch is not `main`, accept it as the launcher-provided worktree.
   Record its actual root/branch and do not compare them with the named
   fallback or create another worktree merely because they differ.
3. Only if the current context is `main`, dirty, unregistered, or otherwise
   unusable should you inspect the named worktree. If that also cannot be
   used, read `.agents.local.env` and require `AGENTS_WORKTREE_CONTAINER_DIR`.
   The planning checkout had
   `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`. Then create a
   unique worktree and branch under that container from pushed
   `origin/main`. Never use `/tmp`, `TMPDIR`, or a guessed path; never
   clean, reset, or discard the original checkout's dirty state. If the
   launcher supplied a dirty or `main` worktree, stop and report it.
4. From the selected worktree, confirm `git rev-parse HEAD` equals
   `git rev-parse origin/main`, confirm
   `git merge-base --is-ancestor aa0c6825e04f82830ed52e0660064dcd68dc4757 HEAD`
   succeeds, and confirm this handoff file exists in the selected `HEAD`.
5. Read `AGENTS.md`, `PAPERCUTS.md`, and the named surfaces.
6. Run cheap orientation only after that decision. Record what you ran.

### While you work

- Keep commits aligned with meaningful chunks, not arbitrary model turns.
- After each chunk, report through the operator: changed files, validation
  actually run, remaining items, new risks, blockers.
- Stop if a contract is missing, intent is ambiguous, scope expands, or
  validation changes the plan.
- Close each finished papercut in `PAPERCUTS.md` with a one-line fix note.

### When the assigned runway is complete

1. Run the required final validation named above.
2. Update `PAPERCUTS.md` so the four items are closed.
3. Push the selected worker branch.
4. Open a reviewable PR against the current pushed `main` tip. The
   planning base SHA is the commit before this handoff, not the commit
   that contains this file.
5. In the PR body, link the four papercuts, changed surfaces, evidence,
   validation, and unresolved items.
6. Report the PR URL to the operator. Do not merge.

### Review and merge path

The orchestrator will review the PR against this handoff, the diff, and
the checks. Current review state: awaiting-review after the PR exists.

Merge remains a separate operator-authorised action.

- **Closeout refs:** `PAPERCUTS.md`; this handoff; the PR.

### Handoff closeout

Leave `PAPERCUTS.md` honest. If an item is already fixed, close it with
proof rather than forcing a diff. If something is blocked, record the
blocker and stop.
