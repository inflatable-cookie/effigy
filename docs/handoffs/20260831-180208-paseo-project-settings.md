---
title: Paseo project settings worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / Northstar orchestrator
created: 2026-08-31
updated: 2026-08-31
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260831-180208-paseo-project-settings.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, paseo]
---

## What This Thread Was Doing

The operator asked the Northstar orchestrator to give every registered project
useful Paseo settings. This lane owns the effigy repository only.

The task is repository-specific configuration, not a request to expand project
planning or mirror every Effigy selector into Paseo.

## Why It Matters

A fresh Paseo worktree should start with the same dependency shape and useful
entry points as the primary checkout. The lifecycle stays portable by calling
Northstar's installed helper through Effigy instead of copying shell or Rhai
code into this repository.

## Current State

- **Repository:** `effigy`
- **Planning branch:** `main`
- **Planning base commit:** `58588f3fbfee80fc04f6b8ac25c49507a1c1fe2e`
- **Pushed main verification:** planning base matched `origin/main` before this
  handoff commit
- **Planning checkout:** clean when dispatched
- **Worker mode:** implementation worker dispatched by the orchestrator
- **Planning artifacts included at the base:** this tracked handoff, committed
  after the planning base
- **Worker branch:** `worker/paseo-project-settings`
- **Worker worktree:** Paseo-managed worktree; record the actual generated path
- **Required sibling worktree links:** none
- **Active spec / roadmap card:** none; this is a bounded operator-authorized
  repository-settings maintenance lane
- **Allowed runway:** inspect, create or update root `paseo.json`, validate,
  commit, push, and open one PR
- **Parallel safety:** no writes outside this repository
- **Canonical refs:** root `AGENTS.md`; repository task/config docs;
  `/Users/tom/.agents/skills/northstar/references/setup/paseo-project.md`
- **Required validation:** JSON parse, installed Northstar lifecycle command smoke checks, `git diff --check`, and the narrowest relevant Effigy QA/docs selectors
- **PR base/head:** `main` ← `worker/paseo-project-settings`
- **Review state:** awaiting worker PR
- **Merge path:** orchestrator after accepted review of the current head and
  passing checks

## Boundaries

- In scope: root `paseo.json` and proof needed to validate it.
- Out of scope: product code, workflows, releases, planning churn, copied
  lifecycle helpers, and unrelated cleanup.
- Use
  `effigy skill run --path "${NORTHSTAR_SKILL_PATH:-$HOME/.agents/skills/northstar}" paseo:worktree -- ...`
  for `prepare`, `link`, and `unlink`.
- Setup order is sibling preparation, a real idempotent repository setup task
  only when needed, then dependency-link replay.
- Add only genuinely useful scripts. Prefer the repository's broad QA, docs QA,
  and at most one canonical dev/preview service. Do not inventory every task.
- Metadata instructions must reflect this repository's live branch, commit, and
  PR conventions.
- Never edit the planning checkout. Never merge the PR.

## Important Context

- Effigy has `qa`, `qa:docs`, and `bootstrap:local`. Treat `bootstrap:local` cautiously because it may mutate the machine-level installed binary; do not put it in automatic worktree setup unless its contract proves that is intended and safe.
- The lifecycle helper discovers primary-checkout dependency links and accepts
  extra relative sibling paths for dependencies not present in that ledger.
- Symlink setup must create absent links, reuse only a symlink resolving to the
  declared source, and stop on any occupied or mismatched path.
- The umbrella Paseo project at `/Users/tom/Dev/projects` is non-Git and does
  not need a ceremonial `paseo.json`.

## Suggested Next Move

Run the worker preflight, read `AGENTS.md`, inspect `effigy tasks --json`,
the root `effigy.toml`, any dependency-link ledger, and any existing
`paseo.json`. Then implement the smallest useful settings file.

## Completion Protocol

### Before work

1. Record `git rev-parse --show-toplevel`, branch, status, and worktree list.
   Accept a clean registered non-`main` launcher worktree and do not create a
   second one.
2. Fetch origin. Confirm `HEAD == origin/main`, the planning base above is an
   ancestor, and this handoff exists in `HEAD`. Load the tracked handoff with
   `git show HEAD:docs/handoffs/20260831-180208-paseo-project-settings.md`.
3. Verify every required sibling link listed above in the worktree container
   directory before broad commands. Stop on missing source, mismatch, or an
   occupied destination.
4. Read the repository instructions and Northstar's Paseo project setup guide.

### Implementation and proof

1. Inspect the live task catalog and dependency topology. Do not assume the
   hints above are complete.
2. Create or update `paseo.json` with the installed lifecycle task, only
   necessary setup, a small useful script set, and project-specific metadata.
3. Validate JSON and lifecycle behavior without deleting or replacing shared
   sibling links. Run the required repository checks.
4. Try to falsify the configuration: clean worktree, missing/mismatched sibling,
   repeat setup, teardown, dependency-link replay, and service-vs-one-shot
   classification where applicable.

### PR and review

Commit the bounded diff, push the worker branch, and open a reviewable PR to
`main`. Explain the chosen setup, omitted shortcuts, sibling handling, and
validation. Report the PR URL and exact head SHA. Do not merge.

If the orchestrator requests changes, stay on this branch, address only those
findings, push, and report again. The orchestrator will send an explicit
follow-up through Paseo so the originating worker is awakened.

