---
title: Papercuts env-lock audit worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy orchestrator
created: 2026-09-01
updated: 2026-09-01
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260901-121205-papercuts-env-lock-audit.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

PR 68 settled catalog-pack acquisition. The next bounded ready Effigy
papercut is the unaudited use of process-global environment variables in
`effigy-containers` tests.

This dispatches one implementation lane. No transcript or second prompt is
part of the authority chain.

## Why It Matters

Tests that read `HOME`, `PATH`, or `EFFIGY_COMPOSE_BACKEND` while sibling tests
mutate them can fail for an unrelated diff. The shared test lock already exists;
the remaining work is to make its precondition complete and proved.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `62d8d6ded9872754158e6e1c16c1aa10137c6f98`
- **Pushed main verification:** the planning commit containing this handoff must
  be pushed and local `HEAD == origin/main` before launch
- **Worker branch:** `worker/papercuts-env-lock-audit`
- **Worker worktree:** launcher-managed
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight
- **Roadmap/spec/card:** none; this is bounded maintenance from `PAPERCUTS.md`
- **Allowed runway:** the `effigy-containers` env-lock papercut only
- **Dispatch topology:** one mechanical implementation lane
- **Ready-frontier shape:** the catalog-listing papercut is also ready but is
  serial behind this lane because both own `PAPERCUTS.md`, evidence, and review
  closeout surfaces
- **Named serial edge:** shared papercut closeout surface
- **Canonical refs:** `AGENTS.md`; contract `001`; `PAPERCUTS.md` open entry
  “`effigy-containers` tests read process-global env without the env lock”
- **Worker class:** mechanical
- **Worker-profile reason:** exhaustive test-reader inventory and repetitive
  guard repair are long mechanical work; no exceptional design reasoning or
  material runtime consequence justifies a frontier implementation worker
- **Tool/runtime restrictions:** no production environment semantics, new
  public API, catalog-pack behavior, publication, release, workflow, S3, or
  unrelated papercut changes
- **Required validation:** focused `effigy-containers` tests under parallel
  stress, repository-owned Effigy QA proportionate to the diff, fmt, clippy,
  and `git diff --check`
- **PR base/head:** current pushed `main` /
  `worker/papercuts-env-lock-audit`
- **Merge path:** orchestrator after accepted exact-head review, passing checks,
  and mergeability

## Boundaries

- **In scope:** inventory every `effigy-containers` test read or mutation of
  `HOME`, `PATH`, and `EFFIGY_COMPOSE_BACKEND`; make the existing
  `crate::test_env_lock()` a complete precondition wherever process-global
  state can overlap; add recurrence proof; close the one papercut; record a
  compact evidence log if repository convention requires it.
- **Preferred repair:** use the existing lock consistently. A thread-local
  redesign is out of scope unless the lock cannot provide a correct proof; stop
  and report that condition rather than widening silently.
- **Out of scope:** production serialization, environment lookup changes,
  unrelated flaky tests, the separate `service list` fragment-count papercut,
  active roadmap/spec/front-door changes, or changelog claims for test-only
  behavior.
- Preserve unrelated work. Work only in the launcher worktree.
- Do not merge the PR. Merge belongs to the orchestrator.

## Review Oracle

Falsify each counterexample and map it to exact proof:

1. A test reads any of the three named variables while another test can mutate
   it without both holding the same lock.
2. A helper hides a named-variable read, so a text-only audit misses an
   unguarded call path.
3. A mutating test restores the variable but releases or bypasses the lock
   before all dependent assertions finish.
4. The repair serializes production code or all container tests rather than
   only tests coupled through process-global state.
5. Repeated parallel focused execution still produces a variable-dependent
   intermittent failure.
6. The papercut is marked closed without naming the audited surface and the
   recurrence proof.

## Suggested Next Move

Run the worker preflight, then read `AGENTS.md`, contract `001`, the selected
papercut, `crates/effigy-containers/src/lib.rs`, and the test modules before
editing.

## Completion Protocol

### Before you start

1. Before broad reads, run `git rev-parse --show-toplevel`,
   `git branch --show-current`, `git status --porcelain`, and
   `git worktree list --porcelain`.
2. Accept a clean, registered, non-`main` launcher worktree. Do not create a
   second worktree because its generated name differs.
3. Run
   `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`.
   Confirm `HEAD == origin/main`, the planning base is an ancestor, and this
   handoff is tracked at `HEAD`. Stop if the absolute file differs.
4. Read the selected refs and run cheap orientation checks.

### While you work

- Inventory semantic reads and mutations, including wrappers, before repair.
- Implement one coherent batch, then validate. Do not run the full suite after
  every small edit.
- Stop on production-behavior changes, a required thread-local redesign, or
  scope expansion.
- Report meaningful chunks through Paseo.

### When the runway is complete

1. Run the required validation and map every oracle row to proof.
2. Move the selected `PAPERCUTS.md` entry to Closed with cause, fix, and
   surface. Do not change active `Next Task` pointers.
3. Commit, push the worker branch, and open one PR against current pushed
   `main`.
4. Report the PR URL, exact head, validation, unresolved items, and docs QA
   classification. Do not merge.

### Review and merge path

The orchestrator reviews the exact PR head and records its verdict on GitHub.
Requested changes return to this worker. Accepted exact head plus passing
checks and mergeability authorizes the orchestrator to merge without another
prompt.

### Handoff closeout

If the audit finds no remaining defect, close the papercut with evidence rather
than inventing a code change. If blocked, record the blocker and stop.
